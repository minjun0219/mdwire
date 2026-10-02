# mdwire

[English](README.md) | 한국어

LLM 이 만든 마크다운을 채팅 채널로 깨지지 않게 보냅니다.
문서 · API · 데모: [mdwire.minjun.dev](https://mdwire.minjun.dev).

에이전트는 마크다운을 출력합니다. 하지만 채팅 채널은 그 마크다운을 그대로 받지 않습니다. 채널마다
받는 문법이 다르고, 이스케이프 규칙이 다르고, 길이 제한이 다릅니다. 기존 변환기는 입력이 잘 짜인
CommonMark 라고 가정하고 한 번에 한 채널만 다룹니다. 에이전트 출력에는 두 가정 모두 맞지 않습니다.

**상태: v0.1.10.** 정규화, 렌더링, 분할, 스트리밍이 여섯 대상에서 동작합니다. Telegram HTML,
Slack `markdown_text`, GitHub 코멘트(GFM), 노션 페이지, 평문, 브라우저 HTML 입니다. 구현은 Rust 코어, CLI, npm 패키지(WASM), Go 구현이 있습니다.
v0.1 에 들어간 것과 일부러 미룬 것은 `SPEC.md` 에 있습니다.

## 하는 일

```
LLM 마크다운  →  정규화  →  채널별 렌더링  →  안전한 분할  →  전송
```

1. **정규화.** 에이전트가 쓴 마크다운은 표준 마크다운 변환으로는 제대로 표현되지 않을 수 있습니다. 짝 없는 `**`, 줄을 접어 쓴 문단에서 줄을
   넘는 강조, 닫히지 않은 코드펜스가 흔합니다. 렌더링하기 전에 고칩니다.
2. **렌더링.** 채널이 실제로 받는 문법으로 출력합니다. Telegram HTML 은 태그 아홉 개만 받고,
   Slack `markdown_text` 는 표준 마크다운을 그대로 받습니다. GitHub 도 표준 마크다운을 받지만 홑 `~` 를
   취소선으로, `<T>` 를 HTML 태그로 읽습니다. 그래서 문자로 쓴 `~`·`<` 는 이스케이프해서(`\~`, `\<`) 출력하고,
   GFM 이 닫지 못하는 강조(`**(a)**` 바로 뒤에 조사가 붙은 경우)는 `<strong>` 으로 출력합니다. 노션은
   그 강조를 제대로 렌더링하지만 인라인 HTML 은 텍스트로 보여 주므로, `notion-markdown` 은 태그를 제거하고
   문자 `*`·`\` 를 이스케이프합니다. 브라우저용 `html` 은 블록까지 태그로 출력하며 `innerHTML` 로 바로
   넣어도 안전합니다. 텍스트는 이스케이프하고, 원문 태그는 속성을 버린 태그 이름만 남기고,
   `http(s)`·`mailto` 링크만 `<a>` 로 만듭니다. 줄바꿈, 이미지, 허용 스킴은 옵션으로 바꿀 수 있습니다.
   스트리밍 중에는 누적 출력에 `preview()`(또는 `closeOpen()`)를 붙이면 항상 짝이 맞는 HTML 이 됩니다.
3. **분할.** 채널의 길이 제한을 지키되 마크업 한가운데를 자르지 않습니다. 스트리밍도 마찬가지로,
   청크 경계가 `**굵게**` 안에 걸리면 안 됩니다.

이 흐름에 옵션이 하나 있습니다. **정규화 보고:** 정규화가 대신 고친 횟수(닫히지 않은 강조,
닫히지 않은 펜스, 짝 없는 백틱, 버린 마커)와 채널에 맞춰 바꿔 쓴 횟수(이스케이프한 글자, 태그로
출력한 강조, 제거한 HTML, 불릿, 표, 바꾼 마커)를 셉니다. 모델이 자기 서식을 얼마나 자주 깨는지 기록하고,
채널을 도입하기 전에 그 채널 때문에 무엇이 바뀌는지 확인할 수 있습니다.

## 사용법

```sh
cat agent-output.md | mdwire --channel telegram-html          # 조각은 NUL 로 구분
cat agent-output.md | mdwire --channel slack-markdown --stream # 받는 대로 바로 출력
cat agent-output.md | mdwire --channel plain --limit 4096      # 텔레그램으로 보내는 평문 폴백

# --report 는 정규화가 고치고 바꾼 것(닫히지 않은 강조, 이스케이프한 `~`, 제거한 태그, …)을
# stderr 에 JSON 한 줄로 출력합니다.
cat agent-output.md | mdwire --channel slack-markdown --report
```

```rust
let parts = mdwire::render(input, Channel::TelegramHtml);

let mut s = Streamer::new(Channel::SlackMarkdown);
s.push_into(chunk, &mut out);  // 청크마다 할당 없음
s.finish_into(&mut out);       // 남은 출력을 내보내고 열린 마크업을 닫음
```

```js
import { render, renderWithReport, Streamer } from "@minjun0219/mdwire";   // npm — 번들러, Node, Bun

const parts = render(markdown, "telegram-html");
const { repairs } = renderWithReport(markdown, "slack-markdown");

// 메시지 전체를 다시 쓰는 채널(텔레그램 edit): acc 에 미리보기를 붙여 보냅니다. 보류 중인
// 내용(열린 굵게, 표의 행, 코드 스팬)을 입력이 여기서 끝난 것처럼 렌더링하며, acc 자체는
// 바꾸지 않습니다. 끝난 뒤 바뀐 것이 없으면 마지막 편집은 건너뜁니다.
const s = new Streamer("telegram-html");
let acc = "";
for await (const chunk of tokens) {
  acc += s.push(chunk);
  await edit(acc + s.preview());
}
acc += s.finish();
if (s.revised()) await edit(acc);

// 덧붙이기만 하는 채널(슬랙 appendStream): push 결과를 그대로 보내고 preview 는 쓰지 않습니다.
const t = new Streamer("slack-markdown");
for await (const chunk of tokens) {
  const piece = t.push(chunk);
  if (piece) await append(piece);
}
await append(t.finish());
```

React 에서는 `@minjun0219/mdwire/react` 가 `createElement` 로 요소를 만들며 `innerHTML` 을
쓰지 않습니다. 이스케이프, 허용 태그, 링크 스킴은 코어의 `html` 채널 한 곳에서 정하고, 태그마다 어떤
컴포넌트로 렌더링할지는 사용하는 쪽이 정합니다. 다른 프레임워크에서는 `@minjun0219/mdwire/events` 로 같은
출력을 `open` / `text` / `close` 이벤트 목록으로 받을 수 있습니다.

```jsx
import { Markdown, useMarkdownStream } from "@minjun0219/mdwire/react";

<Markdown text={answer} components={{ a: RouterLink }} />  // 완성된 답
const { elements, push, finish } = useMarkdownStream();     // 스트리밍: push(토큰), finish()
```

훅은 기본적으로 보류 중인 내용도 먼저 렌더링합니다. `useMarkdownStream({ eager: false })` 를 쓰면 확정된
출력만 보여 주고, `onSettled(html, revised)` 로 완성본이 마지막 화면과 다른지 알려 줍니다.

[`examples/react-streaming`](examples/react-streaming) 은 같은 답변을 react-markdown, Streamdown, Streamdown
앞에 둔 mdwire, mdwire 에 나란히 흘려 비교합니다. 측정 결과는 `DESIGN.md` 에 있습니다.

**append-only 계약.** `push` 가 반환한 출력은 확정입니다. 나중에 들어온 입력이 이미 반환한 출력을
고치지 않고, `finish` 는 뒷부분만 덧붙입니다. 그래서 `push` 결과를 이어 붙이면 입력을 어떤 크기로
끊어 넣었든 한 번에 `render` 한 결과와 같습니다(문서가 길어서 `render` 가 길이 제한에 맞춰 여러 조각으로
나누는 경우는 예외). 테스트 케이스와
퍼징, 그리고 `mdwire-check --scan <dir>` 이 이 계약을 검사합니다. 디렉터리의 모든 파일을 한
글자씩, 64자씩 흘려 보고 결과가 어긋나면 알려 줍니다. `SPEC.md` 8.2절을 참고하세요.

스트리머는 꼭 보류해야 하는 것만 보류합니다. 아직 무엇인지 판단할 수 없는 줄 머리, 청크 끝에 걸린
마커, 아직 닫히지 않은 강조의 안쪽입니다. 문단 전체를 보류하지는 않습니다. 줄바꿈을 기다리는 렌더러는
스트리밍이라고 할 수 없기 때문입니다.

## 왜 또 만들었나

기존 도구에서 실제로 확인한 빈틈은 세 가지입니다.

- **줄을 넘는 강조가 흔합니다.** 에이전트가 만든 문서 60개 중 44개에 줄을 넘는 강조가 있었습니다.
  정규식 기반 변환기는 이를 잘못 짝지어 강조 범위를 *뒤집었고*, 채널은 HTTP 200 을 돌려줘서
  아무도 알아채지 못했습니다.
- **한국어에서 자주 쓰는 표기가 마크다운과 부딪힙니다.**
  - `**설정(config)**을`, `**52%**다` 처럼 굵게가 기호로 끝나고 바로 뒤에 조사가 붙으면, CommonMark
    규칙상 굵게가 닫히지 않아 `**` 가 그대로 보입니다. GitHub 과 브라우저 렌더러가 이 규칙을 따릅니다.
  - `약 ~40km, 5~6월` 처럼 범위나 근사값을 나타내는 물결표가 한 문단에 둘 이상 있으면, 홑 `~` 를
    취소선으로 읽는 GitHub(GFM)은 둘을 짝지어 그 사이(`40km, 5`)에 줄을 긋습니다.

  기존 변환기는 한글 옆 강조에 U+200B 를 끼우거나 그냥 두는 등 제각각이었고, 채널에서 확인해 보지
  않았습니다. mdwire 는 채널마다 직접 측정해서, GitHub 에서는 그 굵게만 `<strong>` 으로 쓰고 글자로 쓴
  `~` 는 `\~` 로 이스케이프합니다.
- **채팅 채널용 변환기는 스트리밍을 고려하지 못합니다.** 브라우저 렌더러 중에는 Streamdown 처럼 스트리밍
  중 닫히지 않은 구문을 보정하는 것도 있습니다. 하지만 살펴본 텔레그램·슬랙용 변환기는 모두 문서
  전체를 받아 한 번에 변환합니다. 토큰이 조금씩 들어오면 청크 경계에서 마크업이 갈라집니다.

## 설계

- **코어에 의존성이 없습니다.** 순수주의 때문이 아닙니다. 일괄 처리 파서는 구조상 스트리밍에 맞지
  않습니다. `DESIGN.md` 를 참고하세요.
- **Rust 코어 하나, 여러 진입점.** npm 은 WASM, CLI 는 정적 바이너리 하나입니다. 가장 중요한 것은 CLI
  입니다. 어떤 언어의 에이전트든 바인딩 없이 파이프로 연결할 수 있습니다. Go 구현은 `go/` 에
  있고(표준 라이브러리만 사용), 같은 테스트 케이스를 통과해야 합니다. Rust 코어와도 직접 대조해서
  무작위 입력과 실제 문서가 똑같이 렌더링되는지 확인합니다.
- **테스트 케이스가 일급 산출물입니다.** `corpus/` 에 채널별 입력과 기대 출력이 있습니다. 다른
  언어로 옮긴 구현은 테스트 케이스를 통과하면 올바른 것입니다. 구현이 여러 개여도 일관성을 지키는
  방법이 바로 이것입니다.

## 설치

레지스트리에서 설치:

```sh
npm install @minjun0219/mdwire     # npm — 번들러, Node, Bun
cargo add mdwire-core              # Rust 라이브러리 (`use mdwire::…`)
cargo install mdwire-cli           # `mdwire` CLI
```

레지스트리를 거치지 않으려면 릴리스마다 첨부되는 산출물을 바로 받아도 됩니다.

```sh
# npm 패키지 (번들러에서도, 번들러 없는 Node 에서도 돈다)
npm install https://github.com/minjun0219/mdwire/releases/download/v0.1.10/mdwire-0.1.10.tgz

# CLI 바이너리 — 플랫폼에 맞는 것 하나
curl -L https://github.com/minjun0219/mdwire/releases/download/v0.1.10/mdwire-v0.1.10-aarch64-apple-darwin.tar.gz | tar xz        # macOS, Apple silicon
curl -L https://github.com/minjun0219/mdwire/releases/download/v0.1.10/mdwire-v0.1.10-x86_64-unknown-linux-gnu.tar.gz | tar xz    # Linux x86_64 (glibc)
curl -L https://github.com/minjun0219/mdwire/releases/download/v0.1.10/mdwire-v0.1.10-aarch64-unknown-linux-gnu.tar.gz | tar xz   # Linux arm64 (glibc)
curl -LO https://github.com/minjun0219/mdwire/releases/download/v0.1.10/mdwire-v0.1.10-x86_64-pc-windows-msvc.zip                 # Windows x86_64
```

릴리스 본문에 산출물마다 SHA-256 이 있습니다. URL 로 설치할 때는 그 값으로 고정하세요.

소스에서 설치: `cargo install --path crates/mdwire-cli`.

Go 는 라이브러리로, 또는 같은 플래그를 받는 CLI 로 설치할 수 있습니다:

```sh
go get github.com/minjun0219/mdwire/go@latest
go install github.com/minjun0219/mdwire/go/cmd/mdwire@latest
```

```go
parts := mdwire.Render(input, mdwire.TelegramHTML)

s := mdwire.NewStreamer(mdwire.SlackMarkdown)
s.PushTo(chunk, &out)   // 청크마다 할당 없음
s.FinishTo(&out)

out := mdwire.RenderWith(input, mdwire.SlackMarkdown, mdwire.Options{})
log.Printf("%+v", out.Repairs)
```

## 코딩 에이전트에게

mdwire 에는 [에이전트 스킬](skills/mdwire/SKILL.md)이 들어 있습니다. 코딩 에이전트에게 언제 mdwire 를 쓸지,
어떤 채널과 진입점을 고를지, 스트리밍은 어떻게 하는지를 알려 줍니다. Agent Skills 형식을 따르므로
`SKILL.md` 를 읽는 에이전트는 그대로 쓸 수 있습니다. Claude Code 에서는 플러그인으로 설치합니다.

```sh
/plugin marketplace add minjun0219/mdwire
/plugin install mdwire@mdwire
```

사이트는 [`llms.txt`](https://mdwire.minjun.dev/llms.txt)도 제공하고, 페이지마다 WebMCP 도구
`mdwire_render` 를 등록합니다. 사이트를 연 브라우저 에이전트가 페이지 안에서 mdwire 를 바로 돌릴 수 있습니다.

## 빌드

```sh
./scripts/build-npm.sh     # npm 패키지를 pkg/ 에 생성 (`cargo install wasm-pack` 필요)
./scripts/smoke.sh         # 빈 프로젝트에 설치해 Node, Bun, TypeScript 에서 불러 봄
```

wasm 바이너리는 `wasm-opt` 를 거친 release 빌드 기준 111 KB 입니다. 패키지에는 빌드가 두 개 들어
있고 `exports` 조건으로 고릅니다. `node` 조건에서는 wasm 을 디스크에서 읽는 CommonJS 빌드를,
그 밖의 환경에서는 번들러용 ESM 빌드를 받습니다. 루트 `package.json` 은 스크립트가 직접 생성합니다.
코어 라이브러리 이름이 이미 `mdwire` 라서 크레이트 이름은 `mdwire-wasm` 이어야 하는데,
wasm-pack 은 npm 패키지 이름을 크레이트 이름에서 가져가기 때문입니다.

```sh
cargo test --workspace     # 단위 테스트, 테스트 케이스 대조, 할당 게이트
cargo clippy --workspace
cargo run --release -p mdwire-bench       # 할당 수와 처리량
cargo run -p mdwire-harness --bin mdwire-check   # 테스트 케이스 + 불변식 채점
```

`mdwire-check` 는 stdin 을 읽어 stdout 에 쓰는 구현이라면 무엇이든 채점합니다. 다른 언어로 옮긴
구현도 같은 기준으로 검사할 수 있습니다.

```sh
mdwire-check --cmd "node convert.js --to {channel}"
mdwire-check --scan ./some-directory-of-markdown   # 기대 출력 없이 불변식만
```

## 릴리스

버전은 아무도 손으로 고치지 않습니다. 배포물(코어 · CLI · wasm · Go 구현의 소스, 매니페스트,
npm 패키징)을 바꾼 커밋이 `main` 에 들어오면 봇이 `release: X.Y.Z` PR 을 열어 둡니다. 그 PR 을 머지하면 `vX.Y.Z` · `go/vX.Y.Z` 태그가 찍히고 산출물이 첨부된 릴리스가
나옵니다. 자세한 내용은 `AGENTS.md` 에 있습니다.

## 라이선스

MIT
