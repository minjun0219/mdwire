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
 * Converts html channel output into an array of React nodes. Use it when you handle the
 * accumulated stream output yourself: `toElements(acc + streamer.closeOpen())`.
 * @param {string} html
 * @param {Partial<Record<string, import("react").ElementType>>} [components] Components that replace individual tags.
 * @param {string[]} [schemes] Schemes allowed for links and images. Same as the `options.html.schemes` given to the core.
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
 * Renders agent markdown, for complete text. If you pass the accumulated stream on every token,
 * it reconverts from scratch each time, so total cost grows quadratically, and half-written markers
 * (`**bo`, an opening backtick, `##`) briefly show up as text and then vanish. For streaming, use [`useMarkdownStream`].
 * @param {import("./react.d.ts").MarkdownProps} props
 */
export function Markdown({ text, components, options }) {
  const html = render(text, "html", options).join("");
  return createElement(Fragment, null, ...toElements(html, components, options?.html?.schemes ?? DEFAULT_SCHEMES));
}

/**
 * Hook for streaming. Call `push(chunk)` as tokens arrive and `finish()` at the end.
 *
 * By default (`eager: true`) it **also draws what is held back**: open emphasis closed, tables with
 * the rows received so far, code spans closed (`Streamer.preview()`). This is a guess, so later tokens
 * may change the shape, but the final output matches a one-shot conversion. With `eager: false`, only
 * final output is shown (append-only, SPEC 8.2): unmatched emphasis and prefixes not yet decided
 * appear once they become final.
 *
 * At the end it calls `onSettled(html, revised)`. `revised` tells whether the final output differs
 * from what was last **drawn on screen**. If they match, the hook does not redraw.
 * @param {import("./react.d.ts").MarkdownStreamOptions} [opts]
 */
export function useMarkdownStream({ components, options, eager = true, onSettled } = {}) {
  const ref = useRef(null);
  const init = useRef(null);
  const [, rerender] = useReducer((n) => n + 1, 0);
  if (init.current === null) {
    init.current = () => ({
      streamer: new Streamer("html", options),
      acc: "",
      done: false,
      eager,
      schemes: options?.html?.schemes ?? DEFAULT_SCHEMES,
    });
  }
  // 해제됐으면 다시 만든다 — StrictMode 개발 모드는 이펙트를 붙였다 떼었다 다시 붙여서, 정리
  // 함수가 스트리머를 먼저 해제한다(토큰이 오기 전이라 잃는 것은 없다).
  const state = () => (ref.current ??= init.current());
  state().onSettled = onSettled; // 콜백은 매번 최신 것을 본다
  // wasm 메모리를 돌려준다 — 스트리머는 JS 가비지 컬렉터가 모르는 곳에 산다.
  useEffect(
    () => () => {
      ref.current?.streamer.free();
      ref.current = null;
    },
    [],
  );
  const push = useCallback((chunk) => {
    const st = state();
    if (st.done) return;
    st.acc += st.streamer.push(chunk);
    rerender();
  }, []);
  const finish = useCallback(() => {
    const st = state();
    if (st.done) return;
    st.acc += st.streamer.finish();
    st.done = true;
    // 스트리머의 revised() 가 아니라 커밋된 화면과 비교한다. 렌더는 버려질 수 있어서(concurrent
    // 렌더) 마지막으로 preview() 를 부른 렌더가 화면에 올라갔다는 보장이 없다.
    const revised = st.acc !== st.shown;
    st.onSettled?.(st.acc, revised);
    if (revised) rerender();
  }, []);
  const st = state();
  const tail = st.done ? "" : st.eager ? st.streamer.preview() : st.streamer.closeOpen();
  const html = st.acc + tail;
  useEffect(() => {
    if (ref.current) ref.current.shown = html;
  });
  return { elements: createElement(Fragment, null, ...toElements(html, components, st.schemes)), push, finish };
}
