# mdwire

Send LLM-generated Markdown to chat channels without it breaking.

Agents emit Markdown. Chat channels don't accept it — each has its own subset, its own
escaping rules, and its own length limit. Existing converters assume the input is
well-formed CommonMark and target one channel at a time. Neither assumption holds for
agent output.

**Status: v0.1 core works.** Normalizing, rendering, splitting and streaming are
implemented for three channels — Telegram HTML, Slack `markdown_text`, and plain text —
with a Rust core, a CLI, and WASM bindings. See `SPEC.md` for what is in v0.1 and what
was deliberately deferred.

## What it does

```
LLM markdown  →  normalize  →  render for channel  →  split safely  →  send
```

1. **Normalize.** Agent output is not well-formed. Unpaired `**`, emphasis that spans a
   line break in wrapped prose, unclosed code fences. Repair before rendering.
2. **Render.** Emit the syntax the channel actually accepts. Telegram HTML allows nine
   tags; Slack `markdown_text` takes standard Markdown directly; legacy `mrkdwn` has no
   headings and no tables.
3. **Split.** Respect the channel's limit — and never cut through markup. This also
   covers streaming: a chunk boundary must not land inside `**bold**`.

## Use it

```sh
cat agent-output.md | mdwire --channel telegram-html          # parts separated by NUL
cat agent-output.md | mdwire --channel slack-markdown --stream # emit as it arrives
```

```rust
let parts = mdwire::render(input, Channel::TelegramHtml);

let mut s = Streamer::new(Channel::SlackMarkdown);
s.push_into(chunk, &mut out);  // no allocation per chunk
s.finish_into(&mut out);       // flush, closing anything left open
```

```js
import { render, Streamer } from "mdwire";   // npm — works under a bundler and in plain Node

const parts = render(markdown, "telegram-html");

const s = new Streamer("telegram-html");
let acc = "";
for await (const chunk of tokens) {
  acc += s.push(chunk);
  await edit(acc + s.closeOpen());   // closeOpen is for sending mid-stream; keep acc as is
}
acc += s.finish();
```

The streamer holds back only what it must: a prefix it cannot classify yet, a marker run
at the end of a chunk, and the inside of an emphasis that has not closed. Paragraphs are
never held — a renderer that waits for a newline is not streaming.

## Why another one

Three gaps in what exists today, each measured rather than assumed:

- **Broken input is the normal case.** In a sample of 60 agent-generated documents,
  44 contained emphasis spanning a line break. A regex-based converter mispaired those
  into *inverted* emphasis ranges — and the channel returned HTTP 200, so nothing caught it.
- **CJK is guessed at.** One converter pads emphasis with U+200B next to Korean text;
  another does not. Neither measured the channel. We did: Slack `markdown_text` follows
  CommonMark, so `_italic_` dies next to a Korean particle and `*italic*` lives. mdwire
  emits `*` and pads nothing.
- **Streaming has no answer.** Every converter is batch: parse the whole document, build an
  AST, render. When tokens arrive incrementally, markup splits across chunk boundaries.

## Design

- **No dependencies in the core.** Not a purity stance — batch parsers are structurally
  wrong for streaming. See `DESIGN.md`.
- **Rust core, many front ends.** WASM for npm, a single static binary for the CLI.
  The CLI matters most: any agent in any language can pipe through it with no bindings.
- **The test corpus is a first-class artifact.** `corpus/` holds input → expected output
  per channel. A port in another language is correct when it passes the corpus. This is
  how consistency survives more than one implementation.

## Installing

Every release carries its own artifacts — no registry needed:

```sh
# npm package (works under a bundler and in plain Node)
npm install https://github.com/minjun0219/mdwire/releases/download/v0.1.1/mdwire-0.1.1.tgz

# CLI binary
curl -L https://github.com/minjun0219/mdwire/releases/download/v0.1.1/mdwire-v0.1.1-aarch64-apple-darwin.tar.gz | tar xz
```

Or from source: `cargo install --path crates/mdwire-cli`.

## Building

```sh
./scripts/build-npm.sh     # the npm package into pkg/ (needs `cargo install wasm-pack`)
./scripts/smoke.sh         # install it into a scratch project and call it from Node + TypeScript
```

The wasm bundle is 87 KB, release with `wasm-opt`. The package carries two builds and
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

## License

MIT
