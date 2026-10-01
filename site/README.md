# site

mdwire 문서·데모 사이트. <https://mdwire.minjun.dev> 에 올린다.

- Astro 정적 사이트. 문서는 MDX(`src/pages/`), 데모만 React island 다.
- 영어 `/`, 한국어 `/ko/` 두 벌이다 — README 가 두 벌인 것과 같은 원칙. 한쪽 페이지를 고치면
  같은 커밋에서 다른 쪽도 고친다.
- **npm 에 올라간 `@minjun0219/mdwire` 를 쓴다.** 로컬 `pkg/` 를 쓰는 `examples/` 와 다르다 —
  배포 빌드(Workers Builds)에는 Rust·wasm-pack 이 없다. 새 버전이 나오면 `package.json` 의
  버전을 올린다.
- 데모 예시(`src/components/samples.ts`)는 `corpus/cases/*/input.md` 를 옮긴 것이다.
- API 문서(`src/pages/docs/`)는 `main` 을 따른다. 코어·바인딩·CLI 의 공개 API 가 바뀌면 같은 PR 에서 문서도
  고친다. 마지막 릴리스에 없는 것에는 `<Next />` 표시를 달고, 릴리스가 나가면 그 표시를 지운다.
- `public/llms.txt` 는 LLM 이 읽을 요약이다([llmstxt.org](https://llmstxt.org) 형식). 채널·API·문서 위치가
  바뀌면 같이 고친다. 버전 번호는 적지 않는다 — 릴리스 봇이 고치지 않는 파일이다.
- 검색 엔진·에이전트용 파일은 빌드 끝에 `integrations/agents.mjs` 가 만든다 — `sitemap.xml`, 개요·문서의
  마크다운 사본(`/docs/npm/` → `/docs/npm.md`), 영어 문서를 한 파일로 모은 `llms-full.txt`. MDX 원본에서
  import · 컴포넌트를 걷어 낸 것이라, 문서에 새 컴포넌트를 쓰면 거기서 마크다운으로 바꾸는 줄도 더한다.
- 링크 미리보기 이미지 `public/og.png`(1200×630)는 손으로 만든 것이다. 채널 목록이 바뀌면 다시 만든다.

패키지 매니저는 pnpm 이다(버전은 `package.json` 의 `packageManager`).

```sh
pnpm install
pnpm run dev           # http://localhost:4321
pnpm run check         # 타입 검사
pnpm run build         # dist/ — 끝에 dist/_version.txt 를 쓴다
pnpm exec wrangler dev # dist/ 를 Workers 와 같은 방식으로 서빙해 본다
```

**번들러에서 mdwire 를 쓸 때** 필요한 설정이 `astro.config.mjs` 에 있다 — `examples/react-streaming`
과 같다. 데모 컴포넌트는 `client:only="react"` 로 싣는다(wasm 을 서버 렌더에서 부르지 않는다).

## 배포

Cloudflare Workers Static Assets 에 Workers Builds(Git 연결)로 올린다. 설정은 `wrangler.jsonc` —
Worker 스크립트 없이 `dist/` 만 올린다. 계정 ID 와 토큰은 저장소에 두지 않는다.

`/_version.txt` 는 빌드한 커밋이다(Workers Builds 는 `WORKERS_CI_COMMIT_SHA`, 로컬은 `git`,
커밋 안 된 변경이 있으면 `-dirty`). 서빙 중인 것이 올린 그것인지 이걸로 본다:

```sh
curl -s https://mdwire.minjun.dev/_version.txt   # 머지한 커밋과 같아야 한다
```

Workers Builds 설정(대시보드):

| 항목 | 값 |
|---|---|
| Root directory | `site` |
| Build command | `pnpm run build` |
| Deploy command | `npx wrangler deploy` (기본값) |
| Preview builds | 켠다 — Preview command 는 기본값 `npx wrangler preview` |
| Build watch paths | 포함 `site/**` — `*` 는 `/` 를 넘지 않아 최상위 파일만 잡는다 |
| Production branch | `main` |

의존성은 Workers Builds 가 빌드 전에 자동으로 설치한다. 배포·프리뷰 명령은 기본값 그대로 둔다 —
Worker Previews 는 Preview command 가 `npx wrangler preview` 를 부르기를 요구하고, `npx` 는 pnpm 이
깐 `node_modules/.bin/wrangler`(`package.json` 의 버전)를 그대로 쓴다.
