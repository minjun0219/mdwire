# site

mdwire 문서·데모 사이트. <https://minjun.kim/mdwire/> 에 올린다 — 개인 사이트의 하위 경로다. 옛 주소 `mdwire.minjun.dev` 도 같은
빌드를 그대로 서빙한다(정본 · canonical 은 새 주소).

- Astro 정적 사이트. 문서는 MDX(`src/pages/`), 데모만 React island 다.
- `base` 가 `/mdwire` 다. 링크는 `getRelativeLocaleUrl` 로 만들면 붙고, 손으로 쓴 절대 경로(`/og.png`)와 `Astro.url.pathname` 을
  읽는 곳은 `src/urls.ts` 의 `withBase` · `stripBase` 를 거친다. MDX 본문의 사이트 안 링크는 `](/mdwire/docs/)` 처럼 base 를
  적어서 쓴다 — Astro 7 의 기본 마크다운 처리기는 플러그인으로 붙여 줄 길이 없다.
- 영어 `/`, 한국어 `/ko/` 두 벌이다 — README 가 두 벌인 것과 같은 원칙. 한쪽 페이지를 고치면
  같은 커밋에서 다른 쪽도 고친다.
- **npm 에 올라간 `@minjun0219/mdwire` 를 쓴다.** 로컬 `pkg/` 를 쓰는 `examples/` 와 다르다 —
  배포 빌드(Workers Builds)에는 Rust·wasm-pack 이 없다. 새 버전이 나오면 `package.json` 의
  버전을 올린다.
- 데모는 `/demo/` 아래 페이지 하나에 하나씩 둔다(목록은 `src/sections.ts`). 예시
  (`src/components/samples.ts`)는 `corpus/cases/*/input.md` 를 옮긴 것이다. 정규화 전후 데모는 그중 npm
  에 올라간 판에서 정규화 보고가 0 이 아닌 케이스만 쓴다.
- 사례(`src/pages/why/cases.mdx`)는 `corpus/cases/` 중 대표 케이스의 입력과 기대 출력을 옮기고 `why.md` 를 풀어 쓴 것이다.
  옮긴 케이스의 기대 출력이 바뀌면 같이 고친다.
- API 문서(`src/pages/docs/`)는 `main` 을 따른다. 코어·바인딩·CLI 의 공개 API 가 바뀌면 같은 PR 에서 문서도
  고친다. 마지막 릴리스에 없는 것에는 `<Next />` 표시를 단다. 릴리스 하루 뒤 `site-bump.yml` 이 사이트의
  npm 판을 올리고 그 판에 들어간 표시를 걷는 PR 을 연다(`scripts/site-release-next.sh`).
- 스트리밍 데모(`src/components/StreamCompare.tsx` · `streamSample.ts`)는 `examples/react-streaming`
  을 옮긴 것이다. 예제의 샘플이나 패널이 바뀌면 같이 고친다. react-markdown · Streamdown 은 데모 페이지만 싣는다.
- `public/llms.txt` 는 LLM 이 읽을 요약이다([llmstxt.org](https://llmstxt.org) 형식). 모든 페이지 푸터에서 링크한다. 채널·API·문서 위치가
  바뀌면 같이 고친다. 버전 번호는 적지 않는다 — 릴리스 봇이 고치지 않는 파일이다.
- 페이지마다 WebMCP 도구 `mdwire_render` 를 등록한다(`src/webmcp.ts`, 레이아웃이 싣는다). 브라우저에 `modelContext` 가
  없으면 아무것도 하지 않고, wasm 은 도구가 처음 불릴 때 받는다. 채널이 바뀌면 도구의 `enum` 도 고친다.
- 방문 집계는 PostHog 다(`src/analytics.ts`, 레이아웃이 싣는다). 키와 프록시 주소는 빌드 변수로 들어온다(아래 배포 절) —
  없는 빌드(로컬 · 프리뷰 · 포크)는 아무것도 하지 않고 SDK 코드도 담지 않는다. 페이지뷰만 집계한다 — 자동 수집 · 세션 녹화 ·
  설문은 끈다.
- 검색 엔진·에이전트용 파일은 빌드 끝에 `integrations/agents.mjs` 가 만든다 — `sitemap.xml`, 개요·문서의
  마크다운 사본(`/docs/npm/` → `/docs/npm.md`), 영어 문서를 한 파일로 모은 `llms-full.txt`. MDX 원본에서
  import 를 걷어 내고 `<Next />` 를 글로 바꾼 것이라, 문서에 새 컴포넌트를 쓰면 거기서 마크다운으로 바꾸는 줄도 더한다.
- 홈은 MDX 본문을 화면용 컴포넌트로 감싼다. `Hero` · `Steps` · `LinkCards` 는 안의 글(slot)을 꾸밀 뿐이라 글은 MDX 에
  그대로 있고, `Showcase`(전후 비교) · `Channels`(채널 카드)는 같은 내용이 본문에 있는 화면용 블록이다. 마크다운 사본은
  컴포넌트만 있는 줄을 걷어 낸다 — 그래서 컴포넌트는 늘 제 줄에 따로 쓴다.
- 홈의 전후 비교(`src/components/Showcase.astro`)는 `corpus/cases/chat-bot-answer/` 의 입력과 채널별 기대 출력을 그대로
  옮긴 정적 화면이다(홈에서 wasm 을 받지 않는다). 그 케이스의 기대 출력이 바뀌면 같이 고친다. 채널 카드
  (`src/components/Channels.astro`)의 이름 · 제한은 문서 개념 페이지의 채널 표와 같다.
- 코드 블록의 복사 버튼과 절 제목의 `#` 링크는 레이아웃(`layouts/Doc.astro`)의 스크립트가 단다. 배경 · 문서 · 데모 묶음의
  페이지 차례는 `src/sections.ts` 한 곳에 있다 — 사이드바와 본문 아래 이전 · 다음 링크가 같이 쓴다.
- 레지스트리 배지(`src/components/Badges.astro`)는 shields.io 이미지라 버전을 손으로 고치지 않는다. 홈은 전부, 언어별
  문서는 그 언어 것만 싣는다. 마크다운 사본에서는 뺀다.
- 링크 미리보기 이미지 `public/og.png`(1200×630)는 손으로 만든 것이다. 채널 목록이 바뀌면 다시 만든다.

한국어 페이지의 본문 글꼴은 Pretendard 다(`pretendard` 패키지, OFL-1.1). `layouts/Doc.astro` 가 유니코드 범위로 나뉜
woff2 를 싣고 `html:lang(ko)` 에만 쓴다 — 영어 페이지는 본문 글꼴을 받지 않는다.
코드 글꼴은 영문이 Menlo(없으면 각 OS 의 고정폭), 한글이 본문과 같은 Pretendard 다 — 글꼴을 따로 더 받지 않는다.
고정폭 표처럼 칸을 맞춰야 하는 블록은 ```` ```text cells ```` 로 쓴다. `astro.config.mjs` 의 shiki 변환기가 전각 글자를
정확히 영문 두 칸(`2ch`)으로 감싸서, 한글 폭이 영문의 두 배가 아닌 글꼴에서도 열이 맞는다.

패키지 매니저는 pnpm 이다(버전은 `package.json` 의 `packageManager`).

```sh
pnpm install
pnpm run dev           # http://localhost:4321
pnpm run check         # 타입 검사
pnpm run build         # dist/mdwire/ — 끝에 dist/mdwire/_version.txt 를 쓴다
pnpm exec wrangler dev # dist/ 를 Workers 와 같은 방식으로 서빙해 본다 — http://localhost:8787/mdwire/
```

**번들러에서 mdwire 를 쓸 때** 필요한 설정이 `astro.config.mjs` 에 있다 — `examples/react-streaming`
과 같다. 데모 컴포넌트는 `client:only="react"` 로 싣는다(wasm 을 서버 렌더에서 부르지 않는다).

## 배포

Cloudflare Workers Static Assets 에 Workers Builds(Git 연결)로 올린다. 설정은 `wrangler.jsonc` —
Worker 스크립트 없이 `dist/` 만 올린다. 계정 ID 와 토큰은 저장소에 두지 않는다.

주소는 `minjun.kim/mdwire/` 다. `minjun.kim` 존의 Workers Routes(`minjun.kim/mdwire` · `minjun.kim/mdwire/*`)가 이 워커로
보낸다 — 개인 사이트 워커의 Custom Domain 앞에서 라우트가 먼저 받는다. 정적 자산은 요청 경로 그대로 찾으므로 산출물은
`dist/mdwire/` 에 나오고(`astro.config.mjs` 의 `base` · `outDir`), `_headers` 만 빌드 끝에 `dist/` 로 올린다.

옛 주소 `mdwire.minjun.dev` 는 Custom Domain 으로 붙여 두고 같은 빌드를 서빙한다. 파일은 `/mdwire/…` 아래에 있으므로 `minjun.dev`
존의 URL Rewrite 규칙(Transform Rule, 대시보드)이 그 호스트의 요청 경로 앞에 `/mdwire` 를 붙인다 — 조건은 호스트가
`mdwire.minjun.dev` 이고 경로가 `/mdwire` 로 시작하지 않을 때. 페이지 안 링크는 `/mdwire/…` 라 옛 호스트에서 누르면
`mdwire.minjun.dev/mdwire/…` 로 가는데, 그 경로는 그대로 파일이 있어 역시 서빙된다. 두 주소를 합치는(301) 건 나중 일이다.
`robots.txt` 는 두지 않는다 — 하위 경로의 것은 읽히지 않는다. 사이트맵은 각 페이지의 `<link rel="sitemap">` 과 개인 사이트의
`robots.txt` 가 가리킨다.

`/mdwire/_version.txt` 는 빌드한 커밋이다(Workers Builds 는 `WORKERS_CI_COMMIT_SHA`, 로컬은 `git`,
커밋 안 된 변경이 있으면 `-dirty`). 서빙 중인 것이 올린 그것인지 이걸로 본다:

```sh
curl -s https://minjun.kim/mdwire/_version.txt   # 머지한 커밋과 같아야 한다
```

Workers Builds 설정(대시보드):

| 항목 | 값 |
|---|---|
| Root directory | `site` |
| Build command | `pnpm run build` |
| Deploy command | `npx wrangler deploy` (기본값) |
| Preview builds | 켠다 — Preview command 는 기본값 `npx wrangler preview`. 프리뷰는 따로 기본 설정(Previews Base configuration)을 쓰니 거기에도 빌드 명령을 넣는다 |
| Build watch paths | 포함 `site/**` (저장소 루트 기준) |
| Production branch | `main` |

프리뷰는 `<브랜치>.mdwire.minjun.dev/mdwire/` 에 뜬다 — base 가 붙은 채로 보면 된다.

빌드 변수(Variables and secrets — 프로덕션과 프리뷰가 따로다). 방문 집계를 켜는 곳은 여기뿐이고, **프로덕션에만 넣는다** —
프리뷰는 보내지 않는다. 둘 중 하나라도 없으면 집계가 꺼진다(스키마는 `astro.config.mjs`).

| 변수 | 값 |
|---|---|
| `POSTHOG_KEY` | PostHog 프로젝트의 공개 키(`phc_…`) |
| `POSTHOG_HOST` | 자체 리버스 프록시 주소(URL) |

빌드 명령은 대시보드에 꼭 넣는다 — `wrangler.jsonc` 의 `build.command` 는 로컬 `wrangler deploy` · `wrangler dev` 용이고,
Workers Builds 는 문서상 그 설정을 따르지 않는다. 의존성은 Workers Builds 가 빌드 전에 자동으로 설치한다. 배포·프리뷰 명령은 기본값 그대로 둔다 —
Worker Previews 는 Preview command 가 `npx wrangler preview` 를 부르기를 요구하고, `npx` 는 pnpm 이
깐 `node_modules/.bin/wrangler`(`package.json` 의 버전)를 그대로 쓴다.
