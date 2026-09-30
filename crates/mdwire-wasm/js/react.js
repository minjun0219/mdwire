// mdwire → React 요소. innerHTML 없이 createElement 로 조립한다.
//
// 안전 판단은 코어의 html 채널이 한다(글자 escape, 태그 고정, 링크 스킴). 여기서는 그 출력을
// 이벤트로 풀어 요소로 세울 뿐이다 — 글자는 React 가 글자로 넣으니 다시 escape 할 것도 없다.
// 링크·이미지 스킴만 한 번 더 본다: 소비자가 `a` 를 자기 컴포넌트로 갈아 끼워도 주소가 안전하도록.
// 목록은 코어에 준 것(`options.html.schemes`)과 같다 — 다르면 코어가 허용한 링크를 여기서 막는다.
import { createElement, Fragment } from "react";
import { render } from "@minjun0219/mdwire";
import { toEvents } from "./events.js";

const DEFAULT_SCHEMES = ["http", "https", "mailto"];

function allowed(url, schemes) {
  const u = url.trimStart();
  const colon = u.indexOf(":");
  if (colon < 1) return false;
  const scheme = u.slice(0, colon).toLowerCase();
  return schemes.some((s) => s.toLowerCase() === scheme);
}
/** 이 안에서는 공백만 있는 글이 DOM 규칙상 자식이 될 수 없다(React 가 경고한다). */
const NO_TEXT = new Set(["ul", "ol", "table", "thead", "tbody", "tr"]);

function propsOf(tag, attrs, key, schemes) {
  const props = { key };
  if (tag === "a" && attrs.href && allowed(attrs.href, schemes)) props.href = attrs.href;
  if (tag === "img" && attrs.src && allowed(attrs.src, schemes)) {
    props.src = attrs.src;
    props.alt = attrs.alt ?? "";
  }
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
 * @param {string[]} [schemes] 링크·이미지로 받는 스킴. 코어에 준 `options.html.schemes` 와 같게.
 */
export function toElements(html, components = {}, schemes = DEFAULT_SCHEMES) {
  const root = { tag: null, children: [] };
  const stack = [root];
  let key = 0;
  const make = (tag, attrs, children) =>
    createElement(components[tag] ?? tag, propsOf(tag, attrs, key++, schemes), ...children);
  for (const ev of toEvents(html)) {
    const top = stack[stack.length - 1];
    switch (ev.type) {
      case "text":
        if (!(NO_TEXT.has(top.tag) && ev.text.trim() === "")) top.children.push(ev.text);
        break;
      case "void":
        // 주소가 안전하지 않은 이미지는 아예 세우지 않는다 — 빈 `<img>` 도 쓸모가 없다.
        if (ev.tag === "img" && !allowed(ev.attrs?.src ?? "", schemes)) break;
        top.children.push(make(ev.tag, ev.attrs ?? {}, []));
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
 * 에이전트 마크다운을 그린다. 스트리밍이면 누적본을 그대로 `text` 로 준다 — 매번 다시
 * 그리지만 결과는 스트리머를 쓴 것과 같다(append-only 계약, SPEC 8.2).
 * @param {import("./react.d.ts").MarkdownProps} props
 */
export function Markdown({ text, from, components, options }) {
  const opts = { ...options, ...(from ? { from } : {}) };
  const html = render(text, "html", opts).join("");
  return createElement(Fragment, null, ...toElements(html, components, opts.html?.schemes ?? DEFAULT_SCHEMES));
}
