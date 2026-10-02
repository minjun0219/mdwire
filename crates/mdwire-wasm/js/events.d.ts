/** Tags that appear in mdwire's html output. */
export type MdTag =
  | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul" | "ol" | "li" | "blockquote" | "pre" | "code"
  | "strong" | "em" | "del" | "a" | "table" | "thead" | "tbody" | "tr" | "th" | "td" | "hr" | "br"
  | "sub" | "sup" | "b" | "i" | "u" | "s" | "strike" | "span" | "small" | "mark" | "kbd" | "ins" | "img";

/**
 * A single event. Attributes hold only what the core emits:
 * `href` on `a`, `class` on `code` (`language-…`), `style` on `th` and `td` (`text-align:…`), `start` on `ol`.
 */
export type MdEvent =
  | { type: "open"; tag: MdTag; attrs: Record<string, string> }
  | { type: "close"; tag: MdTag }
  | { type: "void"; tag: "br" | "hr" }
  /** Images appear only with `html.images: "load"`. Holds `src` and `alt`. */
  | { type: "void"; tag: "img"; attrs: Record<string, string> }
  | { type: "text"; text: string };

/** Parses html channel output (or the accumulated stream output + `closeOpen()`) into an array of events. */
export function toEvents(html: string): MdEvent[];
