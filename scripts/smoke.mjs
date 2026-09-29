// npm 패키지가 실제로 도는지 본다. 빌드만으로는 경계 너머가 안 잡힌다.
// `mdwire` 로 부른다 — `exports` 맵을 거쳐야 진짜 소비자와 같은 길이다.
import { render, limit, Streamer } from "mdwire";
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
for (const channel of ["telegram-html", "slack-markdown", "plain"]) {
  const input = "**굵게**는 먼저\n**안 닫힌 강조가\n다음 줄까지\n\n```ts\nconst a = 1;";
  const st = new Streamer(channel);
  let appended = "";
  for (const ch of input) appended += st.push(ch);
  appended += st.finish();
  assert.equal(appended, render(input, channel).join(""), `${channel}: 이어 붙인 것이 완성본과 다르다`);
}

assert.throws(() => render("x", "없는채널"), /모르는 채널/);
console.log("npm 스모크 통과");
