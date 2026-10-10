// Cloudflare Workers 에서 패키지가 도는지 본다. `smoke.sh` 가 wrangler dev 로 띄워 한 번 부른다.
//
// **번들은 통과하고 첫 호출에서 죽던 고장이다.** `workerd` 조건이 없을 때 wrangler 는 `default`(번들러 빌드)를
// 골랐고, 그 빌드는 `.wasm` import 가 인스턴스를 준다고 가정해 `wasm.__wbindgen_add_to_stack_pointer is not a
// function` 으로 죽었다. 그래서 import 만 보지 않고 실제로 부른다.
import { render, renderWithReport, Streamer } from "@minjun0219/mdwire";
import { toEvents } from "@minjun0219/mdwire/events";

export default {
  async fetch() {
    const s = new Streamer("telegram-html");
    const streamed = s.push("**굵") + s.push("게** 다") + s.finish();
    const got = {
      render: render("**굵게** 다", "telegram-html"),
      report: renderWithReport("**굵게** 다", "telegram-html").parts,
      streamed,
      events: toEvents("<b>x</b>").length > 0,
    };
    const want = { render: ["<b>굵게</b> 다"], report: ["<b>굵게</b> 다"], streamed: "<b>굵게</b> 다", events: true };
    const ok = JSON.stringify(got) === JSON.stringify(want);
    return new Response(JSON.stringify(got), { status: ok ? 200 : 500 });
  },
};
