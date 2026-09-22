// npm 패키지가 실제로 도는지 본다. 빌드만으로는 경계 너머가 안 잡힌다.
import { render, limit, Streamer } from "../pkg-node/mdwire.js";
import { strict as assert } from "node:assert";

assert.equal(limit("telegram-html"), 4096);
assert.deepEqual(render("**굵게** 다", "telegram-html", "auto"), ["<b>굵게</b> 다"]);

const s = new Streamer("telegram-html", "auto");
let acc = "";
for (const chunk of ["**굵", "게** 이어", "서 `코드`"]) {
  acc += s.push(chunk);
  s.closeOpen();   // 중간 송출 경로가 죽지 않는지 — 돌려주는 값은 여기서 안 본다
}
acc += s.finish();
assert.equal(acc, "<b>굵게</b> 이어서 <code>코드</code>");

assert.throws(() => render("x", "없는채널", "auto"), /모르는 채널/);
console.log("npm 스모크 통과");
