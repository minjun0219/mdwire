# AGENTS.md

## 이 저장소가 푸는 문제

에이전트가 만든 마크다운을 채팅 채널로 내보낼 때 깨지지 않게 한다. 자세한 근거와
실측은 `DESIGN.md`.

## 규칙

- **코어에 의존성을 추가하지 않는다.** `crates/mdwire-core`는 std만 쓴다. 의존이
  필요하면 바인딩·CLI 층에 둔다. 기성 파서를 끌어오자는 제안은 `DESIGN.md`의
  "파싱 범위는 출력이 정한다"를 먼저 읽는다.
- **코퍼스가 정본이다.** 동작을 바꾸면 `corpus/`에 케이스를 더한다. 실제로 겪은
  고장만 넣는다.
- **성능이 기능보다 앞선다.** 스트리밍 경로는 조각마다 불린다. 할당과 복사를 센다.
- 커밋·PR 제목은 Conventional Commits + 한국어 요약. 설명 주석도 한국어.
- 공개 저장소다. 개인 경로·내부 도구 이름·다른 저장소 이야기를 남기지 않는다.
- **README 는 영어(`README.md`)·한국어(`README.ko.md`) 두 벌이 같이 간다.** 한쪽을 고치면
  같은 커밋에서 다른 쪽도 고친다. 절 구성·코드 블록·링크가 어긋나면 CI 가 멈춘다
  (`scripts/readme-sync.sh`).

## 브랜치

`main` 에는 PR 로만 들어간다(브랜치 룰셋). 머지는 squash 로 — `main` 에 머지 커밋을
만들지 않는다.

게이트는 CI(`ci.yml`)와 같다. 통과 전에 커밋하지 않는다.

```sh
./scripts/readme-sync.sh
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings
cargo build -p mdwire-wasm --target wasm32-unknown-unknown
cargo build -p mdwire-cli && (cd go && test -z "$(gofmt -l .)" && go vet ./... \
  && MDWIRE_RUST=../target/debug/mdwire go test ./...)   # 없으면 러스트 대조가 조용히 빠진다
./scripts/build-npm.sh && CI=1 ./scripts/smoke.sh          # Node · Bun · TypeScript
(cd examples/react-streaming && npm install && npm run build)  # 예제는 pkg/ 를 쓴다
(cd site && pnpm install --frozen-lockfile && pnpm run check && pnpm run build)  # site/ 를 바꿨을 때
```

Go 이식의 기대값은 Rust CLI에서 뽑는다(`go/testdata/regen.sh`) — 코어 동작을 바꾸면 다시 뽑는다.

## 지도

- `crates/mdwire-core` — 코어(std만, lib 이름 `mdwire`). crates.io 게시
- `crates/mdwire-cli` — `mdwire` CLI. crates.io 게시
- `crates/mdwire-wasm` — npm `@minjun0219/mdwire`의 바인딩(`scripts/build-npm.sh`)
- `crates/mdwire-harness` — 코퍼스 대조·불변식 채점(`mdwire-check`), `crates/mdwire-bench` — 할당·처리량
- `go/` — Go 이식, `corpus/` — 정본 케이스
- `site/` — 문서·데모 사이트(Astro, 독립 패키지). npm 에 올라간 패키지를 쓴다. 배포는 `site/README.md`

## 릴리스

버전·`CHANGELOG.md`·README 설치 줄의 버전(릴리스 URL · 상태 줄)은 손으로 고치지 않는다. main 에 **배포물을 바꾼**
커밋(코어·CLI·wasm·Go 이식의 소스, 매니페스트·잠금, npm 패키징 — 시험 제외)이 들어오면
`release-pr.yml` 이 `release: X.Y.Z` PR 을 열어 두고(사이트·예제·문서·시험·CI 만 바꾼 커밋으로는 안 연다), 그걸 squash 로 머지하는 것이
릴리스다 — 머지된 커밋에서 바이너리 검증(`binaries.yml`, 네 플랫폼 빌드·실행·묶기)이 통과하면
`vX.Y.Z` · `go/vX.Y.Z` 태그와 GitHub Release 가 따라 나온다. 버전은 패치만
자동으로 오른다. 다른 버전은 main 의 `[workspace.package] version` 으로 정한다.
crates.io(`mdwire-core` · `mdwire-cli`)와 npm(`@minjun0219/mdwire`)도 `release.yml` 이 Trusted
Publishing 으로 올린다 — 저장소에 토큰을 두지 않는다. 레지스트리에 아직 없는 이름의 첫 판만
사람이 토큰으로 올린다.
