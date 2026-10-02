---
name: mdwire
description: Send LLM- or agent-generated Markdown to a chat channel without it breaking. Repairs broken markup (unclosed ** or code fences), rewrites it into the syntax the channel accepts (Telegram HTML, Slack markdown_text, GitHub comments, Notion, plain text, or HTML safe for innerHTML) and splits it within the channel's length limit, also while tokens stream. Use when a bot or agent posts model output to Telegram, Slack, GitHub or Notion; when Telegram answers "Bad Request: can't parse entities"; when a streamed answer shows stray ** or half-drawn formatting; when a message is over Telegram's 4,096 characters; or when rendering streaming LLM Markdown in React without innerHTML.
license: MIT
compatibility: Node, Bun or a bundler (npm @minjun0219/mdwire, WASM); Rust (mdwire-core); Go (github.com/minjun0219/mdwire/go); any other language through the mdwire CLI.
---

# mdwire

mdwire sits between the model and the channel: LLM Markdown → normalize → render for the channel →
split safely → send. It does not draw anything; the channel still renders. Docs: https://mdwire.minjun.dev
(every docs page is also Markdown at the same path plus `.md`, e.g. https://mdwire.minjun.dev/docs/npm.md).

## Do not hand-roll this

Regex converters and "escape everything" helpers fail on real model output: emphasis that wraps across
a line break gets paired with the wrong marker (the range is inverted and the channel still returns
200), an unclosed `**` or fence from a cut-off answer breaks Telegram's parser, and splitting at 4,096
characters lands inside a tag. Pass the model's Markdown to mdwire as is — do not pre-escape it and do
not run it through another Markdown converter first.

## Pick the channel

| Channel | Send it as | Limit |
|---|---|---|
| `telegram-html` | Telegram `sendMessage` / `editMessageText` with `parse_mode: "HTML"` | 4,096 |
| `slack-markdown` | Slack `chat.postMessage` `markdown_text` (not `text`/mrkdwn) | 12,000 |
| `github-markdown` | GitHub issue/PR comment or body | 65,536 |
| `notion-markdown` | Notion page body (Notion-flavored Markdown) | 65,536 |
| `plain` | Anywhere without formatting; fallback | 12,000 |
| `html` | Browser `innerHTML` (escaped; only http/https/mailto links) | none |

Each returned part is one message. Send them in order.

## Pick the entry point

- **Node, Bun, browser, bundler** → `npm install @minjun0219/mdwire`. Under Vite add `vite-plugin-wasm`
  and `build.target: "esnext"`.
- **Rust** → `cargo add mdwire-core` (imported as `mdwire`).
- **Go** → `go get github.com/minjun0219/mdwire/go@latest`.
- **Python or anything else** → the CLI: `cargo install mdwire-cli`, or
  `go install github.com/minjun0219/mdwire/go/cmd/mdwire@latest`, or a binary from
  https://github.com/minjun0219/mdwire/releases.

## One finished answer

```ts
import { render } from "@minjun0219/mdwire";

for (const part of render(answer, "telegram-html")) {
  await bot.sendMessage(chatId, part, { parse_mode: "HTML" });
}
```

```python
import subprocess

out = subprocess.run(["mdwire", "--channel", "telegram-html"],
                     input=answer, capture_output=True, text=True, check=True).stdout
parts = out.split("\0")  # parts are NUL-separated
```

```go
parts := mdwire.Render(answer, mdwire.TelegramHTML)
```

```rust
let parts = mdwire::render(&answer, mdwire::Channel::TelegramHtml);
```

## Streaming tokens

`push` returns only what is safe to send now and never rewrites it. Choose by how the channel updates:

```ts
import { Streamer } from "@minjun0219/mdwire";

// The channel redraws the whole message (Telegram editMessageText): accumulated output + preview().
const s = new Streamer("telegram-html");
let acc = "";
for await (const token of tokens) {
  acc += s.push(token);
  await edit(acc + s.preview()); // throttle edits; preview() draws held text as if the input ended here
}
acc += s.finish();
if (s.revised()) await edit(acc);
s.free();

// The channel only appends (Slack appendStream): send each push() piece; never call preview().
const t = new Streamer("slack-markdown");
for await (const token of tokens) {
  const piece = t.push(token);
  if (piece) await append(piece);
}
await append(t.finish());
t.free();
```

Streaming does not split. If a streamed Telegram message can pass 4,096 characters, stream into the
message and, after `finish()`, send the final document again with `render` to get the parts.

## React

```tsx
import { Markdown, useMarkdownStream } from "@minjun0219/mdwire/react";

<Markdown text={answer} components={{ a: RouterLink }} />   // finished answer, no innerHTML
const { elements, push, finish } = useMarkdownStream();       // streaming: push(token), finish()
```

## Gotchas

- `renderWithReport` results, their `repairs`, and `Streamer` live in WASM memory: call `free()` (or `using`).
- `render` is enough when you do not need the repair report.
- `limit` below 256 is raised to 256; every part needs room to close and reopen markup.
- The repair report (`renderWithReport`, `mdwire --report`) counts how often the model broke its own
  formatting — useful to log per answer. `guessedPair` counts emphasis mdwire paired by a guess
  (`값**(합계)**를`); a non-zero count marks an answer worth a look.
