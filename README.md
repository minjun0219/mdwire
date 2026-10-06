# mdwire

English | [한국어](README.ko.md)

Send LLM-generated Markdown to chat channels without it breaking.
Docs, API reference and a live demo: [minjun.kim/mdwire](https://minjun.kim/mdwire/).

Agents emit Markdown. Chat channels don't take it as is — each has its own subset, its own
escaping rules, and its own length limit. Existing converters assume the input is
well-formed CommonMark and target one channel at a time. Neither assumption holds for
agent output.

**Status: v0.1.13.** Normalizing, rendering, splitting and streaming work for six
targets — Telegram HTML, Slack `markdown_text`, GitHub comments (GFM), Notion pages, plain text,
and HTML for the browser — from a Rust core, a CLI,
an npm package (WASM), and a Go port. See `SPEC.md` for what is in v0.1 and what was
deliberately deferred. `SPEC.md` and `DESIGN.md` are written in Korean.

## What it does

```
LLM markdown  →  normalize  →  render for channel  →  split safely  →  send
```

1. **Normalize.** Agent-written Markdown may not render correctly through a standard
   Markdown converter. Typical cases: unpaired `**`, emphasis that spans a line break in wrapped
   prose, unclosed code fences. Repair before rendering.
2. **Render.** Emit the syntax the channel actually accepts. Telegram HTML allows nine
   tags; Slack `markdown_text` takes standard Markdown directly, with a `*` meant as a character
   escaped (`\*`) so Slack does not re-read it as emphasis. GitHub takes it too, but
   reads a lone `~` as strikethrough and `<T>` as an HTML tag — so a `~` or `<` meant as a
   character goes out escaped (`\~`, `\<`), and emphasis that GFM would not close (`**(a)**`
   followed directly by a Korean particle) goes out as `<strong>`. Notion renders that bold as is
   but shows inline HTML as text, so `notion-markdown` strips the tags and escapes a literal `*`
   or `\` instead. For the browser, `html` renders
   blocks as tags too and is safe to set as `innerHTML`: text is escaped, inline tags from the
   source keep no attributes, and only `http(s)`/`mailto` links become `<a>` — line breaks,
   images and allowed schemes are options. While streaming,
   the accumulated output plus `preview()` (or `closeOpen()`) is always balanced HTML.
3. **Split.** Respect the channel's limit — and never cut through markup. This also
   covers streaming: a chunk boundary must not land inside `**bold**`.

The pipeline has one option. **Repair report:** how many times the normalizer stepped
in — unclosed emphasis, unclosed fence, unpaired backticks, dropped markers, guessed pairs — and what it
rewrote for the channel — escaped characters, tags for emphasis, stripped HTML, bullets,
tables, converted markers — so you can log how often the model breaks its own formatting
and see what a channel changes before you adopt it.

## Use it

```sh
cat agent-output.md | mdwire --channel telegram-html          # parts separated by NUL
cat agent-output.md | mdwire --channel slack-markdown --stream # emit as it arrives
cat agent-output.md | mdwire --channel plain --limit 4096      # plain fallback into Telegram

# --report prints what the normalizer fixed and rewrote (unclosed emphasis, escaped `~`,
# stripped tags, …) to stderr as one JSON line.
cat agent-output.md | mdwire --channel slack-markdown --report
```

```rust
let parts = mdwire::render(input, Channel::TelegramHtml);

let mut s = Streamer::new(Channel::SlackMarkdown);
s.push_into(chunk, &mut out);  // no allocation per chunk
s.finish_into(&mut out);       // flush, closing anything left open
```

```js
import { render, renderWithReport, Streamer } from "@minjun0219/mdwire";   // npm — bundlers, Node, Bun

const parts = render(markdown, "telegram-html");
const { repairs } = renderWithReport(markdown, "slack-markdown");

// A channel that rewrites the whole message (Telegram edit): send acc plus the preview —
// what is still held (open bold, table rows, a code span) drawn as if the input ended here.
// Keep acc itself untouched. After finish, skip the last edit if nothing changed.
const s = new Streamer("telegram-html");
let acc = "";
for await (const chunk of tokens) {
  acc += s.push(chunk);
  await edit(acc + s.preview());
}
acc += s.finish();
if (s.revised()) await edit(acc);

// An append-only channel (Slack appendStream): send each piece as is — never preview.
const t = new Streamer("slack-markdown");
for await (const chunk of tokens) {
  const piece = t.push(chunk);
  if (piece) await append(piece);
}
await append(t.finish());
```

In React, `@minjun0219/mdwire/react` builds elements with `createElement` — no
`innerHTML`. Escaping, the tag set and link schemes are decided once, in the core's `html`
channel; you choose which component draws each tag. `@minjun0219/mdwire/events` gives the
same output as an `open` / `text` / `close` event list for other frameworks. A screen that draws
Markdown with `@lezer/markdown` or CodeMirror can use `@minjun0219/mdwire/lezer` instead, which makes
that parser read emphasis the way mdwire does.

```jsx
import { Markdown, useMarkdownStream } from "@minjun0219/mdwire/react";

<Markdown text={answer} components={{ a: RouterLink }} />  // a finished answer
const { elements, push, finish } = useMarkdownStream();     // streaming: push(token), finish()
```

The hook draws held content early by default; `useMarkdownStream({ eager: false })` shows only
what is final, and `onSettled(html, revised)` tells you whether the finished text differs from
the last frame.

[`examples/react-streaming`](examples/react-streaming) streams one answer into react-markdown, Streamdown,
mdwire in front of Streamdown, and mdwire side by side. Measured numbers are in `DESIGN.md`.

**Append-only contract.** What `push` returns is final — a later chunk never rewrites it —
and `finish` only appends the tail. So the pieces concatenated equal a one-shot `render`,
whatever the chunk size (unless the document is long enough to be split into parts).
This is tested on the corpus, by fuzzing, and by `mdwire-check --scan <dir>`, which streams
every file one character and 64 characters at a time and reports any divergence. See
`SPEC.md` §8.2.

The streamer holds back only what it must: a prefix it cannot classify yet, a marker run
at the end of a chunk, and the inside of an emphasis that has not closed. A whole paragraph is
never held — a renderer that waits for a newline is not streaming.

## Why another one

We found three gaps in existing tools:

- **Emphasis spanning lines is common.** In a sample of 60 agent-generated documents,
  44 contained emphasis spanning a line break. A regex-based converter mispaired those
  into *inverted* emphasis ranges — and the channel returned HTTP 200, so nothing caught it.
- **Common Korean notation collides with Markdown.**
  - In `**설정(config)**을` or `**52%**다`, the bold ends in a symbol and a particle follows right
    after. Under CommonMark's rules the bold does not close and the `**` shows as text. GitHub and
    browser renderers follow those rules.
  - In `약 ~40km, 5~6월`, tildes mark an approximation and a range. With two of them in one
    paragraph, GitHub (GFM), which reads a single `~` as strikethrough, pairs them and strikes
    everything in between (`40km, 5`).

  The two converters we compared disagree on Korean-adjacent emphasis (one pads it with U+200B, the
  other leaves it alone), and neither checked the channel. mdwire measured each channel: on GitHub it writes just that
  bold as `<strong>` and escapes a literal `~` as `\~`.
- **Chat-channel converters ignore streaming.** Some browser renderers, like Streamdown, patch
  unclosed syntax while tokens stream in. The Telegram and Slack converters we looked at all
  take the whole document and convert it in one pass. When tokens arrive incrementally,
  markup splits across chunk boundaries.

## Design

- **No dependencies in the core.** Not a purity stance — batch parsers are structurally
  wrong for streaming. See `DESIGN.md`.
- **Rust core, many front ends.** WASM for npm, a single static binary for the CLI.
  The CLI matters most: any agent in any language can pipe through it with no bindings.
  A Go port lives in `go/` (stdlib only) and is held to the same corpus — and to the Rust
  core itself: random inputs and real documents must render identically.
- **The test corpus is a first-class artifact.** `corpus/` holds input → expected output
  per channel. A port in another language is correct when it passes the corpus. This is
  how consistency survives more than one implementation.

## Installing

From the registries:

```sh
npm install @minjun0219/mdwire     # npm — bundlers, Node, Bun
cargo add mdwire-core              # Rust library (`use mdwire::…`)
cargo install mdwire-cli           # the `mdwire` CLI
```

Every release also carries its own artifacts, if you would rather not go through a registry:

```sh
# npm package (works under a bundler and in plain Node)
npm install https://github.com/minjun0219/mdwire/releases/download/v0.1.13/mdwire-0.1.13.tgz

# CLI binary — pick your platform
curl -L https://github.com/minjun0219/mdwire/releases/download/v0.1.13/mdwire-v0.1.13-aarch64-apple-darwin.tar.gz | tar xz        # macOS, Apple silicon
curl -L https://github.com/minjun0219/mdwire/releases/download/v0.1.13/mdwire-v0.1.13-x86_64-unknown-linux-gnu.tar.gz | tar xz    # Linux x86_64 (glibc)
curl -L https://github.com/minjun0219/mdwire/releases/download/v0.1.13/mdwire-v0.1.13-aarch64-unknown-linux-gnu.tar.gz | tar xz   # Linux arm64 (glibc)
curl -LO https://github.com/minjun0219/mdwire/releases/download/v0.1.13/mdwire-v0.1.13-x86_64-pc-windows-msvc.zip                 # Windows x86_64
```

The release notes list a SHA-256 for every artifact — verify the download against it when installing by URL.

Or from source: `cargo install --path crates/mdwire-cli`.

Go, as a library or a CLI with the same flags:

```sh
go get github.com/minjun0219/mdwire/go@latest
go install github.com/minjun0219/mdwire/go/cmd/mdwire@latest
```

```go
parts := mdwire.Render(input, mdwire.TelegramHTML)

s := mdwire.NewStreamer(mdwire.SlackMarkdown)
s.PushTo(chunk, &out)   // no allocation per chunk
s.FinishTo(&out)

out := mdwire.RenderWith(input, mdwire.SlackMarkdown, mdwire.Options{})
log.Printf("%+v", out.Repairs)
```

## For coding agents

mdwire ships an [agent skill](skills/mdwire/SKILL.md) that tells a coding agent when to reach for it,
which channel and entry point to pick, and how to stream. It follows the Agent Skills format, so agents
that read `SKILL.md` can use it directly. In Claude Code it installs as a plugin:

```sh
/plugin marketplace add minjun0219/mdwire
/plugin install mdwire@mdwire
```

The site also serves [`llms.txt`](https://minjun.kim/mdwire/llms.txt), and its pages register a WebMCP
tool, `mdwire_render`, so a browser agent on the site can run mdwire in the page.

## Building

```sh
./scripts/build-npm.sh     # the npm package into pkg/ (needs `cargo install wasm-pack`)
./scripts/smoke.sh         # install it into a scratch project; call it from Node, Bun, and TypeScript
```

The wasm binary is 111 KB (release build, after `wasm-opt`). The package carries two builds and
picks by `exports` condition: `node` gets a CommonJS build that loads the wasm from disk,
everything else gets the ESM bundler build. The script writes the root `package.json`
itself: the crate has to stay `mdwire-wasm` because the core's library is already named
`mdwire`, and wasm-pack takes the npm name from the crate.

```sh
cargo test --workspace     # unit tests, the corpus, and the allocation gate
cargo clippy --workspace
cargo run --release -p mdwire-bench       # allocation counts and throughput
cargo run -p mdwire-harness --bin mdwire-check   # corpus + invariant scoring
```

`mdwire-check` scores any implementation that reads stdin and writes stdout, so a port in
another language can be measured with the same yardstick:

```sh
mdwire-check --cmd "node convert.js --to {channel}"
mdwire-check --scan ./some-directory-of-markdown   # invariants only, no expected output
```

## Releasing

Nobody edits the version by hand. When a merge to `main` changes what ships (the core, CLI,
WASM or Go sources, manifests, npm packaging), a bot keeps a `release: X.Y.Z` pull request open; merging it tags `vX.Y.Z` and `go/vX.Y.Z` and publishes
the release with its artifacts. See `AGENTS.md`.

## License

MIT
