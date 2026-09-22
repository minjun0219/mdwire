// 타입만 본다 — 실행하지 않는다. `smoke.sh` 가 nodenext 설정으로 tsc 를 돌린다.
import { render, limit, Streamer } from "mdwire";

const parts: string[] = render("**굵게**", "slack-markdown", "auto");
const n: number = limit("slack-markdown");
const s = new Streamer("slack-markdown", "auto");
const out: string = s.push("조각") + s.closeOpen() + s.finish();
void [parts, n, out];
