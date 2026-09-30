// mdwire → React 요소. innerHTML 없이 createElement 로 조립한다.
//
// 안전 판단은 코어의 html 채널이 한다(글자 escape, 태그 고정, 링크 스킴). 여기서는 그 출력을
// 이벤트로 풀어 요소로 세울 뿐이다 — 글자는 React 가 글자로 넣으니 다시 escape 할 것도 없다.
// 링크 스킴만 한 번 더 본다: 소비자가 `a` 를 자기 컴포넌트로 갈아 끼워도 주소가 안전하도록.
import { createElement, Fragment } from "react";
import { render } from "@minjun0219/mdwire";
import { toEvents } from "./events.js";

const SAFE_HREF = /^\s*(https?:\/\/|mailto:)/i;
/** 이 안에서는 공백만 있는 글이 DOM 규칙상 자식이 될 수 없다(React 가 경고한다). */
const NO_TEXT = new Set(["ul", "ol", "table", "thead", "tbody", "tr"]);

function propsOf(tag, attrs, key) {
  const props = { key };
  if (tag === "a" && attrs.href && SAFE_HREF.test(attrs.href)) props.href = attrs.href;
  if (tag === "code" && attrs.class) props.className = attrs.class;
  if ((tag === "th" || tag === "td") && attrs.style) {
    const align = /^text-align:(left|right|center)$/.exec(attrs.style);
    if (align) props.style = { textAlign: align[1] };
  }
  if (tag === "ol" && attrs.start) {
    const n = Number(attrs.start);
    if (Number.isInteger(n) && n > 0) props.start = n;
  }
  return props;
}

/**
 * html 채널 출력을 React 노드 배열로 바꾼다. 스트리밍 누적본을 직접 다루는 쪽이 쓴다 —
 * `toElements(acc + streamer.closeOpen())`.
 * @param {string} html
 * @param {Partial<Record<string, import("react").ElementType>>} [components] 태그별로 갈아 끼울 컴포넌트.
 */
export function toElements(html, components = {}) {
  const root = { tag: null, children: [] };
  const stack = [root];
  let key = 0;
  const make = (tag, attrs, children) =>
    createElement(components[tag] ?? tag, propsOf(tag, attrs, key++), ...children);
  for (const ev of toEvents(html)) {
    const top = stack[stack.length - 1];
    switch (ev.type) {
      case "text":
        if (!(NO_TEXT.has(top.tag) && ev.text.trim() === "")) top.children.push(ev.text);
        break;
      case "void":
        top.children.push(make(ev.tag, {}, []));
        break;
      case "open":
        stack.push({ tag: ev.tag, attrs: ev.attrs, children: [] });
        break;
      case "close": {
        // 코어 출력은 짝이 맞는다. 안 맞는 닫기는 버린다 — 남이 넣은 글이라도 트리는 선다.
        if (top.tag !== ev.tag || stack.length === 1) break;
        stack.pop();
        stack[stack.length - 1].children.push(make(top.tag, top.attrs, top.children));
        break;
      }
    }
  }
  // 안 닫힌 것은 닫는다(스트리밍 중 closeOpen 을 안 붙인 누적본).
  while (stack.length > 1) {
    const top = stack.pop();
    stack[stack.length - 1].children.push(make(top.tag, top.attrs, top.children));
  }
  return root.children;
}

/**
 * 에이전트 마크다운을 그린다 — 완성된 글용. **스트리밍 중인 누적본을 토큰마다 넘기지 않는다** — 매번
 * 처음부터 다시 변환해 전체 비용이 제곱으로 는다. 스트리밍은 `Streamer` 로 새 토큰만 변환하고
 * `toElements(acc + streamer.closeOpen())` 으로 세운다(append-only 계약, SPEC 8.2).
 * @param {import("./react.d.ts").MarkdownProps} props
 */
export function Markdown({ text, from, components }) {
  const html = render(text, "html", from ? { from } : undefined).join("");
  return createElement(Fragment, null, ...toElements(html, components));
}
