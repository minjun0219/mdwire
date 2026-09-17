# mdwire

Send LLM-generated Markdown to chat channels without it breaking.

Agents emit Markdown. Chat channels don't accept it — each has its own subset, its own
escaping rules, and its own length limit. Existing converters assume the input is
well-formed CommonMark and target one channel at a time. Neither assumption holds for
agent output.

**Status: design stage. Not usable yet.**

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

## Why another one

Three gaps in what exists today, each measured rather than assumed:

- **Broken input is the normal case.** In a sample of 60 agent-generated documents,
  44 contained emphasis spanning a line break. A regex-based converter mispaired those
  into *inverted* emphasis ranges — and the channel returned HTTP 200, so nothing caught it.
- **CJK is an afterthought.** One converter pads emphasis with U+200B next to Korean text;
  another does not. The same input renders differently per channel, and neither is
  configurable.
- **Streaming has no answer.** Every converter is batch: parse the whole document, build an
  AST, render. When tokens arrive incrementally, markup splits across chunk boundaries.

## Design

- **No dependencies in the core.** Not a purity stance — batch parsers are structurally
  wrong for streaming, and CJK emphasis policy is baked into theirs where we need it
  configurable. See `DESIGN.md`.
- **Rust core, many front ends.** WASM for npm, a single static binary for the CLI.
  The CLI matters most: any agent in any language can pipe through it with no bindings.
- **The test corpus is a first-class artifact.** `corpus/` holds input → expected output
  per channel. A port in another language is correct when it passes the corpus. This is
  how consistency survives more than one implementation.

## License

MIT
