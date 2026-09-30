// npm 패키지가 실제로 도는지 본다. 빌드만으로는 경계 너머가 안 잡힌다.
// `mdwire` 로 부른다 — `exports` 맵을 거쳐야 진짜 소비자와 같은 길이다.
import { render, renderWithReport, limit, Streamer } from "@minjun0219/mdwire";
import { strict as assert } from "node:assert";

assert.equal(limit("telegram-html"), 4096);
assert.deepEqual(render("**굵게** 다", "telegram-html"), ["<b>굵게</b> 다"]);

const s = new Streamer("telegram-html");
let acc = "";
for (const chunk of ["**굵", "게** 이어", "서 `코드`"]) {
  acc += s.push(chunk);
  s.closeOpen();   // 중간 송출 경로가 죽지 않는지 — 돌려주는 값은 여기서 안 본다
}
acc += s.finish();
assert.equal(acc, "<b>굵게</b> 이어서 <code>코드</code>");

// **append-only 계약.** `push` 가 돌려준 글은 확정분이다 — 뒤 조각이 앞 글을 고치지 않는다.
// 그래서 받은 대로 이어 붙이기만 하면(Slack `appendStream` 처럼 앞 글을 못 고치는 채널)
// 한 번에 렌더한 것과 같다. 깨진 입력(안 닫힌 강조·펜스)을 한 글자씩 흘려서 본다.
for (const channel of ["telegram-html", "slack-markdown", "github-markdown", "plain", "html"]) {
  const input = "**굵게**는 먼저\n**안 닫힌 강조가\n다음 줄까지\n\n```ts\nconst a = 1;";
  const st = new Streamer(channel);
  let appended = "";
  for (const ch of input) appended += st.push(ch);
  appended += st.finish();
  assert.equal(appended, render(input, channel).join(""), `${channel}: 이어 붙인 것이 완성본과 다르다`);
}

// 입력 방언 — 레거시 mrkdwn 으로 쓴 에이전트 출력. 옵션은 객체 하나다.
assert.deepEqual(
  render("*굵게* ~취소~ <https://x.io|링크>", "slack-markdown", { from: "slack-mrkdwn" }),
  ["**굵게** ~~취소~~ [링크](https://x.io)"],
);
// 한도는 호출자가 정한다 — plain 폴백을 텔레그램으로 보낼 때 4096.
const long = "가나다 ".repeat(3000);
assert.ok(render(long, "plain").length === 1, "plain 기본 한도는 12,000 이다");
const capped = render(long, "plain", { limit: 4096 });
assert.ok(capped.length > 1 && capped.every((p) => [...p].length <= 4096), "limit 을 넘는 조각이 있다");
assert.throws(() => render(long, "plain", { limit: 0 }), /limit/);
const ms = new Streamer("telegram-html", { from: "slack-mrkdwn" });
assert.equal(ms.push("*굵") + ms.push("게*") + ms.finish(), "<b>굵게</b>");
assert.throws(() => render("x", "plain", { from: "mrkdwn" }), /모르는 방언/);

// 정규화가 고친 것.
const report = renderWithReport("**영향 범위\n```ts\nconst a = 1;", "telegram-html");
assert.equal(report.parts.length, 1);
assert.equal(report.repairs.closedEmphasis, 1);
assert.equal(report.repairs.closedFence, 1);
const rs = new Streamer("slack-markdown");
rs.push("**열고 안 닫힘");
rs.finish();
assert.equal(rs.repairs().closedEmphasis, 1);

assert.throws(() => render("x", "없는채널"), /모르는 채널/);
console.log("npm 스모크 통과");

// ── 구조 출력(이벤트)과 React ────────────────────────────────────────────
// html 출력을 이벤트로 푼다. 코어가 낸 태그만 태그로 읽고, 속성은 코어가 내는 것만 담긴다.
import { toEvents } from "@minjun0219/mdwire/events";
const ev = toEvents(render("**굵게** 와 [링크](https://a.com) 1 < 2", "html").join(""));
assert.deepEqual(ev, [
  { type: "open", tag: "p", attrs: {} },
  { type: "open", tag: "strong", attrs: {} },
  { type: "text", text: "굵게" },
  { type: "close", tag: "strong" },
  { type: "text", text: " 와 " },
  { type: "open", tag: "a", attrs: { href: "https://a.com" } },
  { type: "text", text: "링크" },
  { type: "close", tag: "a" },
  { type: "text", text: " 1 < 2" },
  { type: "close", tag: "p" },
]);

// React — createElement 로만 세운다. 정적 렌더로 모양을 본다.
const { createElement } = await import("react");
const { renderToStaticMarkup } = await import("react-dom/server");
const { Markdown, toElements } = await import("@minjun0219/mdwire/react");
const md = (text, props = {}) => renderToStaticMarkup(createElement(Markdown, { text, ...props }));
assert.equal(md("## 제목\n\n- **하나**\n- 둘"), "<h2>제목</h2>\n\n<ul><li><strong>하나</strong>\n</li><li>둘</li></ul>");
// 스크립트가 도는 길은 없다 — 원문 태그의 속성은 버려지고, javascript: 는 링크가 아니다.
assert.equal(md('H<sub onclick="x()">2</sub>O [x](javascript:alert(1))'), "<p>H<sub>2</sub>O x (javascript:alert(1))</p>");
// 태그별로 컴포넌트를 갈아 끼운다 — 링크를 앱의 라우터 링크로.
const Link = ({ href, children }) => createElement("span", { "data-href": href }, children);
assert.equal(md("[문서](https://a.com/d)", { components: { a: Link } }), '<p><span data-href="https://a.com/d">문서</span></p>');
// 스트리밍 — 누적본 + closeOpen 을 그대로 요소로.
const hs = new Streamer("html");
const hacc = hs.push("> 인용이 **굵게 이어");
assert.equal(renderToStaticMarkup(createElement("div", null, ...toElements(hacc + hs.closeOpen()))), "<div><blockquote>인용이 </blockquote></div>");
