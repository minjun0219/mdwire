# mdwire

[English](README.md) | 한국어

LLM 이 만든 마크다운을 채팅 채널로 깨지지 않게 보낸다.

에이전트는 마크다운을 낸다. 채팅 채널은 그걸 그대로 받지 않는다 — 채널마다 받는 문법이
다르고, escape 규칙이 다르고, 길이 한도가 다르다. 기존 변환기는 입력이 잘 짜인
CommonMark 라고 가정하고 한 번에 한 채널만 본다. 에이전트 출력에는 두 가정이 다 맞지
않는다.

**상태: v0.1.6.** 정규화·렌더·분할·스트리밍이 세 채널 — Telegram HTML, Slack
`markdown_text`, 평문 — 에서 돈다. Rust 코어, CLI, npm 패키지(WASM), Go 이식이 있다.
v0.1 에 든 것과 일부러 미룬 것은 `SPEC.md` 에 있다.

## 하는 일

```
LLM 마크다운  →  정규화  →  채널용 렌더  →  안전한 분할  →  발송
```

1. **정규화.** 에이전트 출력은 잘 짜여 있지 않다. 짝 없는 `**`, 줄바꿈된 문단에서 줄을
   넘는 강조, 안 닫힌 코드펜스. 렌더 전에 고친다.
2. **렌더.** 채널이 실제로 받는 문법으로 낸다. Telegram HTML 은 태그 아홉 개만 받고,
   Slack `markdown_text` 는 표준 마크다운을 그대로 받는다.
3. **분할.** 채널 한도를 지키되 마크업 한가운데를 자르지 않는다. 스트리밍도 같다 —
   조각 경계가 `**굵게**` 안에 떨어지면 안 된다.

이 흐름에 옵션이 둘 붙는다. **입력 방언:** 슬랙 문서로 슬랙을 배운 에이전트는 레거시
`mrkdwn`(`*굵게*`, `~취소~`, `<url|텍스트>`)으로 쓴다. `--from slack-mrkdwn` 을 주면 표준
마크다운이 아니라 그 표기로 읽는다. **고친 것 보고:** 정규화가 대신 고친 횟수 — 안 닫힌
강조, 안 닫힌 펜스, 짝 없는 백틱, 버린 마커. 모델이 제 서식을 얼마나 자주 깨는지 로그로
잴 수 있다.

## 쓰기

```sh
cat agent-output.md | mdwire --channel telegram-html          # 조각은 NUL 로 구분
cat agent-output.md | mdwire --channel slack-markdown --stream # 들어오는 대로 내보낸다

# 에이전트가 슬랙 레거시 mrkdwn(*굵게*, ~취소~)으로 썼다면 그렇다고 알려 준다. --report 는
# 정규화가 고친 것(안 닫힌 강조, 안 닫힌 펜스, …)을 stderr 에 JSON 한 줄로 낸다.
cat agent-output.md | mdwire --channel slack-markdown --from slack-mrkdwn --report
```

```rust
let parts = mdwire::render(input, Channel::TelegramHtml);

let mut s = Streamer::new(Channel::SlackMarkdown);
s.push_into(chunk, &mut out);  // 조각마다 할당 없음
s.finish_into(&mut out);       // 남은 것을 내보내고 열린 것을 닫는다
```

```js
import { render, renderWithReport, Streamer } from "@minjun0219/mdwire";   // npm — 번들러, Node, Bun

const parts = render(markdown, "telegram-html");
const { repairs } = renderWithReport(markdown, "slack-markdown", { from: "slack-mrkdwn" });

// 메시지 전체를 고쳐 쓰는 채널(텔레그램 edit): acc 에 열린 블록을 닫는 꼬리를 붙여
// 보낸다. acc 자체는 건드리지 않는다.
const s = new Streamer("telegram-html");
let acc = "";
for await (const chunk of tokens) {
  acc += s.push(chunk);
  await edit(acc + s.closeOpen());
}
acc += s.finish();

// 이어 붙이기만 하는 채널(슬랙 appendStream): 받은 조각을 그대로 보낸다 — closeOpen 은 쓰지 않는다.
const t = new Streamer("slack-markdown");
for await (const chunk of tokens) {
  const piece = t.push(chunk);
  if (piece) await append(piece);
}
await append(t.finish());
```

**append-only 계약.** `push` 가 돌려준 것은 확정이다 — 뒤 조각이 그걸 고쳐 쓰지 않는다 —
그리고 `finish` 는 꼬리만 덧붙인다. 그래서 조각을 이어 붙인 것은 조각 크기와 상관없이 한
번에 `render` 한 결과와 같다(문서가 길어 여러 조각으로 나뉘는 경우는 빼고). 코퍼스와
퍼즈, 그리고 `mdwire-check --scan <dir>` 이 이것을 본다 — 디렉터리의 파일을 전부 한
글자씩, 64자씩 흘려 보고 어긋나면 알린다. Node, Bun, 번들러에서 돈다. `SPEC.md` 8.2절.

스트리머는 꼭 붙들어야 하는 것만 붙든다. 아직 무엇인지 가릴 수 없는 줄머리, 조각 끝에
걸린 마커, 아직 안 닫힌 강조의 안쪽. 문단은 붙들지 않는다 — 줄바꿈을 기다리는 렌더러는
스트리밍이 아니다.

## 왜 또 만드나

지금 있는 것들의 빈틈 셋. 짐작이 아니라 잰 것이다.

- **깨진 입력이 보통이다.** 에이전트가 만든 문서 60개 중 44개에 줄을 넘는 강조가 있었다.
  정규식 변환기는 이걸 잘못 짝지어 강조 범위를 *뒤집었고*, 채널은 HTTP 200 을 돌려줘서
  아무도 몰랐다.
- **한글 처리는 짐작이다.** 어떤 변환기는 한글 옆 강조에 U+200B 를 끼우고, 어떤 것은
  안 끼운다. 둘 다 채널을 재 보지 않았다. 우리가 쟀다. Slack `markdown_text` 는
  CommonMark 를 따라서 한글 조사 옆 `_기울임_` 은 죽고 `*기울임*` 은 산다. mdwire 는
  `*` 을 쓰고 아무것도 끼우지 않는다.
- **스트리밍에 답이 없다.** 변환기가 전부 일괄 처리다. 문서 전체를 파싱해 AST 를 만들고
  렌더한다. 토큰이 조금씩 들어오면 마크업이 조각 경계에서 갈린다.

## 설계

- **코어에 의존성이 없다.** 순수주의가 아니다 — 일괄 파서는 구조상 스트리밍에 맞지
  않는다. `DESIGN.md` 참고.
- **Rust 코어, 여러 앞단.** npm 은 WASM, CLI 는 정적 바이너리 하나. CLI 가 가장 중요하다
  — 어느 언어의 에이전트든 바인딩 없이 파이프로 통과시킬 수 있다. Go 이식은 `go/` 에
  있고(표준 라이브러리만), 같은 코퍼스를 통과해야 한다. Rust 코어 자체와도 대조한다 —
  무작위 입력과 실제 문서가 똑같이 렌더돼야 한다.
- **테스트 코퍼스가 일급 산출물이다.** `corpus/` 에 채널별 입력 → 기대 출력이 있다. 다른
  언어의 이식은 코퍼스를 통과하면 맞는 것이다. 구현이 여럿이어도 일관성이 유지되는
  방법이 이것이다.

## 설치

레지스트리에서:

```sh
npm install @minjun0219/mdwire     # npm — 번들러, Node, Bun
cargo add mdwire-core              # Rust 라이브러리 (`use mdwire::…`)
cargo install mdwire-cli           # `mdwire` CLI
```

레지스트리를 거치지 않으려면 릴리스마다 붙는 산출물을 바로 받아도 된다.

```sh
# npm 패키지 (번들러에서도, 맨 Node 에서도 돈다)
npm install https://github.com/minjun0219/mdwire/releases/download/v0.1.6/mdwire-0.1.6.tgz

# CLI 바이너리 — macOS(Apple silicon) 또는 Linux(x86_64)
curl -L https://github.com/minjun0219/mdwire/releases/download/v0.1.6/mdwire-v0.1.6-aarch64-apple-darwin.tar.gz | tar xz
curl -L https://github.com/minjun0219/mdwire/releases/download/v0.1.6/mdwire-v0.1.6-x86_64-unknown-linux-gnu.tar.gz | tar xz
```

릴리스 본문에 산출물마다 SHA-256 이 있다 — URL 로 설치할 때 그 값으로 고정한다.

소스에서: `cargo install --path crates/mdwire-cli`.

Go — 라이브러리로, 또는 같은 플래그를 받는 CLI 로:

```sh
go get github.com/minjun0219/mdwire/go@latest
go install github.com/minjun0219/mdwire/go/cmd/mdwire@latest
```

```go
parts := mdwire.Render(input, mdwire.TelegramHTML)

s := mdwire.NewStreamer(mdwire.SlackMarkdown)
s.PushTo(chunk, &out)   // 조각마다 할당 없음
s.FinishTo(&out)

out := mdwire.RenderWith(input, mdwire.SlackMarkdown, mdwire.Options{From: mdwire.SlackMrkdwn})
log.Printf("%+v", out.Repairs)
```

## 빌드

```sh
./scripts/build-npm.sh     # npm 패키지를 pkg/ 에 만든다 (`cargo install wasm-pack` 필요)
./scripts/smoke.sh         # 빈 프로젝트에 설치해 Node, Bun, TypeScript 에서 불러 본다
```

wasm 바이너리는 `wasm-opt` 를 거친 release 빌드로 111 KB 다. 패키지에는 빌드가 둘 들어
있고 `exports` 조건으로 고른다. `node` 조건은 wasm 을 디스크에서 읽는 CommonJS 빌드를,
그 밖에는 번들러용 ESM 빌드를 받는다. 루트 `package.json` 은 스크립트가 직접 쓴다 —
코어 라이브러리 이름이 이미 `mdwire` 라서 크레이트는 `mdwire-wasm` 이어야 하는데,
wasm-pack 은 npm 이름을 크레이트 이름에서 가져가기 때문이다.

```sh
cargo test --workspace     # 단위 테스트, 코퍼스, 할당 게이트
cargo clippy --workspace
cargo run --release -p mdwire-bench       # 할당 수와 처리량
cargo run -p mdwire-harness --bin mdwire-check   # 코퍼스 + 불변식 채점
```

`mdwire-check` 는 stdin 을 읽어 stdout 에 쓰는 구현이면 무엇이든 채점한다. 다른 언어의
이식도 같은 잣대로 잴 수 있다.

```sh
mdwire-check --cmd "node convert.js --to {channel}"
mdwire-check --scan ./some-directory-of-markdown   # 불변식만, 기대 출력 없이
```

## 릴리스

버전은 아무도 손으로 고치지 않는다. `main` 에 머지될 때마다 봇이 `release: X.Y.Z` PR 을
열어 두고, 그걸 머지하면 `vX.Y.Z` · `go/vX.Y.Z` 태그가 찍히고 산출물이 붙은 릴리스가
나온다. `AGENTS.md` 참고.

## 라이선스

MIT
