/** mdwire html 출력에 나오는 태그. */
export type MdTag =
  | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul" | "ol" | "li" | "blockquote" | "pre" | "code"
  | "strong" | "em" | "del" | "a" | "table" | "thead" | "tbody" | "tr" | "th" | "td" | "hr" | "br"
  | "sub" | "sup" | "b" | "i" | "u" | "s" | "strike" | "span" | "small" | "mark" | "kbd";

/**
 * 이벤트 하나. 속성은 코어가 내는 것만 담긴다 —
 * `a` 의 `href`, `code` 의 `class`(`language-…`), `th`·`td` 의 `style`(`text-align:…`), `ol` 의 `start`.
 */
export type MdEvent =
  | { type: "open"; tag: MdTag; attrs: Record<string, string> }
  | { type: "close"; tag: MdTag }
  | { type: "void"; tag: "br" | "hr" }
  | { type: "text"; text: string };

/** html 채널 출력(또는 스트리밍 누적본 + `closeOpen()`)을 이벤트 배열로 푼다. */
export function toEvents(html: string): MdEvent[];
