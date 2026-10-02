// `@minjun0219/mdwire/lezer` — 설치한 패키지의 lezer 확장이 코어와 같은 답을 내는지 본다.
import assert from "node:assert/strict";
import { GFM, parser } from "@lezer/markdown";
import { render } from "@minjun0219/mdwire";
import { koreanEmphasis } from "@minjun0219/mdwire/lezer";

// 트리를 텔레그램 HTML 과 같은 모양으로 적는다 — 마커 노드는 지우고 강조·코드만 태그로.
function text(doc, node) {
  let out = "";
  let pos = node.from;
  for (let c = node.firstChild; c; c = c.nextSibling) {
    out += doc.slice(pos, c.from) + show(doc, c);
    pos = c.to;
  }
  return out + doc.slice(pos, node.to);
}
function show(doc, n) {
  switch (n.name) {
    case "EmphasisMark":
    case "StrikethroughMark":
    case "CodeMark":
      return "";
    case "StrongEmphasis":
      return `<b>${text(doc, n)}</b>`;
    case "Emphasis":
      return `<i>${text(doc, n)}</i>`;
    case "Strikethrough":
      return `<s>${text(doc, n)}</s>`;
    case "InlineCode":
      return `<code>${text(doc, n)}</code>`;
    default:
      return text(doc, n);
  }
}
const read = (p, doc) => show(doc, p.parse(doc).topNode);

const p = parser.configure([GFM, koreanEmphasis]);

// 코어(텔레그램 HTML)와 같아야 하는 것들.
const same = [
  "**설정(config)**을 바꾼다",
  '**「캐시」**가 · **"배포 금지"**는 · **끝.**이라서 · **52%**다',
  "*설정(config)*을 ~~예전(구)~~는",
  "***중요(필수)***를",
  '①**"주간 졸림"**',
  "_진료_가 있다",
  "snake_case_name 은 그대로",
  "값**(합계)**를 본다",
  "2**(n-1) 은 거듭제곱",
  "x**(y)**z 와 2**(n-1)**2",
  "x**(y)**z 와 값**(합계)**를",
  "2**(n-1) (**주의**) 를 본다",
  "2**(n-1) 【**주의**】 값)**를 본다**.",
  "카드 4***-****-****-003* 번호, underfront.* (4개), 2 ** 3",
  "** 배포 ** 는 금지",
  "**굵게 *기울임* 끝**",
  "**`코드`**였다",
  "**마통**이 · **(중요)** 다",
  "앞 **굵게**\n다음 줄 **굵게2** 끝",
];
for (const input of same) {
  assert.equal(read(p, input), render(input, "telegram-html").join(""), input);
}
// 코어는 짝 잃은 `**` 를 버리지만, 이 확장은 글자로 둔다 — 범위는 같다.
assert.equal(read(p, "채널**이다. 글\n**신분 공개**이"), "채널**이다. 글\n<b>신분 공개</b>이");
// 진 여는 마커는 물린다 — 뒤의 짝 잃은 마커와 다시 짝지어 문단 전체가 굵어지면 안 된다.
assert.equal(read(p, "**old\n**new** tail**"), "**old\n<b>new</b> tail**");
// 쌓는 순서 둘 다 된다.
assert.equal(read(parser.configure(GFM).configure(koreanEmphasis), "**설정(config)**을"), "<b>설정(config)</b>을");
// GFM 없이도 강조는 되고, 취소선은 글자로 남는다(노드가 없다).
const bare = parser.configure(koreanEmphasis);
assert.equal(read(bare, "**설정(config)**을 ~~x~~가"), "<b>설정(config)</b>을 ~~x~~가");
console.log("lezer 확장 통과");
