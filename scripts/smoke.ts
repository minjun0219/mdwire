// 타입만 본다 — 실행하지 않는다. `smoke.sh` 가 nodenext 설정으로 tsc 를 돌린다.
import { render, renderWithReport, limit, Streamer } from "@minjun0219/mdwire";
import type { RenderOptions } from "@minjun0219/mdwire";

const parts: string[] = render("**굵게**", "slack-markdown");
const n: number = limit("slack-markdown");
const s = new Streamer("slack-markdown");
const out: string = s.push("조각") + s.closeOpen() + s.finish();

// 옵션은 좁은 타입이다 — 방언 이름을 틀리면 컴파일에서 걸린다.
const opts: RenderOptions = { from: "slack-mrkdwn" };
const withOpts: string[] = render("*굵게*", "slack-markdown", opts);
const report = renderWithReport("**열림", "telegram-html", opts);
const closed: number = report.repairs.closedEmphasis + s.repairs().closedFence;
const reportParts: string[] = report.parts;
void [parts, n, out, withOpts, closed, reportParts];

// 구조 출력과 React — 서브패스 타입이 선다.
import { toEvents } from "@minjun0219/mdwire/events";
import type { MdEvent } from "@minjun0219/mdwire/events";
import { Markdown, toElements } from "@minjun0219/mdwire/react";
import type { MarkdownProps } from "@minjun0219/mdwire/react";
const events: MdEvent[] = toEvents("<p>x</p>");
const props: MarkdownProps = { text: "**x**", from: "slack-mrkdwn", components: { a: "span" } };
void [events, Markdown(props), toElements("<p>x</p>")];
