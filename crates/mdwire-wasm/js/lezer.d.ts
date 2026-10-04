import type { MarkdownConfig } from "@lezer/markdown";

/**
 * A `@lezer/markdown` extension that reads emphasis the way mdwire does: `**설정(config)**을` — bold followed
 * directly by a Korean particle — closes instead of leaving the asterisks as text. It replaces the built-in
 * `Emphasis` parser and GFM's `Strikethrough` parser by name, so put it after them:
 * `parser.configure([GFM, koreanEmphasis])`; for CodeMirror,
 * `markdown({ base: markdownLanguage, extensions: [koreanEmphasis] })`.
 *
 * Markers that pair become `StrongEmphasis` / `Emphasis` / `Strikethrough` nodes; markers that do not pair stay as
 * text. The rules are documented at https://minjun.kim/mdwire/docs/emphasis/.
 */
export declare const koreanEmphasis: MarkdownConfig;
