// mdwire 의 html 채널 출력을 이벤트 열로 푼다.
//
// **mdwire 가 낸 HTML 만 읽는다.** 코어가 글자를 전부 escape 하고 태그를 고정된 집합으로만
// 내므로(SPEC 4절) 여기서 필요한 것은 작은 토크나이저 하나다. 남이 쓴 HTML 을 넣으면 모르는
// 태그는 글자로 돌려준다 — 이 모듈은 새니타이저가 아니다. 안전 판단은 코어 한 곳에 있다.

/** 출력에 나올 수 있는 태그. 이 밖의 것은 글자다. */
const TAGS = new Set([
  "p", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "li", "blockquote", "pre", "code",
  "strong", "em", "del", "a", "table", "thead", "tbody", "tr", "th", "td", "hr", "br",
  "sub", "sup", "b", "i", "u", "s", "strike", "span", "small", "mark", "kbd", "ins", "img",
]);
const VOID = new Set(["br", "hr", "img"]);
/** 태그마다 읽는 속성. 코어가 내는 것만 — 나머지는 버린다. */
const ATTRS = { a: ["href"], code: ["class"], th: ["style"], td: ["style"], ol: ["start"], img: ["src", "alt"] };

const ENTITIES = { amp: "&", lt: "<", gt: ">", quot: '"' };
const decode = (s) => s.replace(/&(amp|lt|gt|quot);/g, (_, n) => ENTITIES[n]);

/**
 * html 채널 출력을 이벤트 배열로 바꾼다.
 * @param {string} html `render(text, "html")` 의 결과, 또는 스트리밍 누적본 + `closeOpen()`.
 * @returns {import("./events.d.ts").MdEvent[]}
 */
export function toEvents(html) {
  const events = [];
  let text = "";
  const flush = () => {
    if (text) {
      events.push({ type: "text", text: decode(text) });
      text = "";
    }
  };
  let i = 0;
  while (i < html.length) {
    const lt = html.indexOf("<", i);
    if (lt < 0) {
      text += html.slice(i);
      break;
    }
    text += html.slice(i, lt);
    const gt = html.indexOf(">", lt);
    const m = gt < 0 ? null : /^<(\/?)([a-z][a-z0-9]*)((?:\s+[a-z-]+="[^"]*")*)\s*\/?>$/.exec(html.slice(lt, gt + 1));
    if (!m || !TAGS.has(m[2])) {
      text += "<";
      i = lt + 1;
      continue;
    }
    flush();
    const [, closing, tag, rawAttrs] = m;
    if (closing) {
      events.push({ type: "close", tag });
    } else if (VOID.has(tag)) {
      const attrs = {};
      for (const [, name, value] of rawAttrs.matchAll(/([a-z-]+)="([^"]*)"/g)) {
        if (ATTRS[tag]?.includes(name)) attrs[name] = decode(value);
      }
      events.push(tag === "img" ? { type: "void", tag, attrs } : { type: "void", tag });
    } else {
      const attrs = {};
      for (const [, name, value] of rawAttrs.matchAll(/([a-z-]+)="([^"]*)"/g)) {
        if (ATTRS[tag]?.includes(name)) attrs[name] = decode(value);
      }
      events.push({ type: "open", tag, attrs });
    }
    i = gt + 1;
  }
  flush();
  return events;
}
