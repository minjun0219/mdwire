// mdwire → React 요소. innerHTML 없이 createElement 로 조립한다.
//
// 안전 판단은 코어의 html 채널이 한다(글자 escape, 태그 고정, 링크 스킴). 여기서는 그 출력을
// 이벤트로 풀어 요소로 세울 뿐이다 — 글자는 React 가 글자로 넣으니 다시 escape 할 것도 없다.
// 링크·이미지 스킴만 한 번 더 본다: 소비자가 `a` 를 자기 컴포넌트로 갈아 끼워도 주소가 안전하도록.
// 목록은 코어에 준 것(`options.html.schemes`)과 같다 — 다르면 코어가 허용한 링크를 여기서 막는다.
import { createElement, Fragment, useCallback, useEffect, useReducer, useRef } from "react";
import { render, Streamer } from "@minjun0219/mdwire";
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
 * 에이전트 마크다운을 그린다 — 완성된 글용. 스트리밍 중인 누적본을 토큰마다 넘기면 매번
 * 처음부터 다시 변환해 전체 비용이 제곱으로 늘고, 반쪽 마커(`**굵`, 여는 백틱, `##`)가 잠깐 글자로
 * 보였다가 사라진다. 스트리밍은 [`useMarkdownStream`].
 * @param {import("./react.d.ts").MarkdownProps} props
 */
export function Markdown({ text, from, components, options }) {
  const opts = { ...options, ...(from ? { from } : {}) };
  const html = render(text, "html", opts).join("");
  return createElement(Fragment, null, ...toElements(html, components, opts.html?.schemes ?? DEFAULT_SCHEMES));
}

/**
 * 스트리밍용 훅. 토큰이 오는 대로 `push(chunk)`, 끝나면 `finish()`.
 *
 * 기본(`eager: true`)은 **붙든 것도 먼저 그린다** — 열린 강조는 닫아서, 표는 지금까지 온 행으로,
 * 코드 스팬은 닫아서(`Streamer.preview()`). 추측이라 뒤 토큰이 모양을 바꿀 수 있지만 완성본은
 * 일괄 변환과 같다. `eager: false` 면 확정된 것만 보인다(append-only, SPEC 8.2) — 짝이 안 맞은
 * 강조·판정 전 접두사는 확정될 때 나온다.
 *
 * 끝나면 `onSettled(html, revised)` — `revised` 는 완성본이 마지막 화면과 다른가다. 같으면 훅은
 * 다시 그리지 않는다.
 * @param {import("./react.d.ts").MarkdownStreamOptions} [opts]
 */
export function useMarkdownStream({ from, components, options, eager = true, onSettled } = {}) {
  const ref = useRef(null);
  const [, rerender] = useReducer((n) => n + 1, 0);
  if (ref.current === null) {
    const o = { ...options, ...(from ? { from } : {}) };
    ref.current = { streamer: new Streamer("html", o), acc: "", done: false, eager, schemes: o.html?.schemes ?? DEFAULT_SCHEMES };
  }
  // 콜백은 매번 최신 것을 본다 — 옵션처럼 처음 한 번만 읽으면 닫힌 값이 남는다.
  ref.current.onSettled = onSettled;
  // wasm 메모리를 돌려준다 — 스트리머는 JS 가비지 컬렉터가 모르는 곳에 산다.
  useEffect(() => () => ref.current?.streamer.free(), []);
  const push = useCallback((chunk) => {
    const st = ref.current;
    if (st.done) return;
    st.acc += st.streamer.push(chunk);
    rerender();
  }, []);
  const finish = useCallback(() => {
    const st = ref.current;
    if (st.done) return;
    st.acc += st.streamer.finish();
    st.done = true;
    // 미리보기를 안 썼으면 늘 참이다 — 마지막 화면은 확정분 + closeOpen 이었다.
    const revised = st.streamer.revised();
    st.onSettled?.(st.acc, revised);
    if (revised) rerender();
  }, []);
  const st = ref.current;
  const tail = st.done ? "" : st.eager ? st.streamer.preview() : st.streamer.closeOpen();
  return { elements: createElement(Fragment, null, ...toElements(st.acc + tail, components, st.schemes)), push, finish };
}
