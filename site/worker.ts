// 같은 주소에서 HTML 과 마크다운 사본을 `Accept` 로 고른다. 사본이 있는 페이지(`src/markdownCopies.ts`)만
// `wrangler.jsonc` 의 `run_worker_first` 가 여기로 보내고, 나머지 요청은 Worker 를 거치지 않고 정적 자산이 받는다.
//
// 마크다운을 주는 건 `Accept` 에 `text/markdown` 이 직접 적혀 있고, 그 q 가 0 보다 크며 `text/html` 에 적용되는
// q 이상일 때다. 동점은 마크다운이다 — Claude Code 의 WebFetch 가 `text/markdown, text/html, */*` 로 보낸다.
// `*/*`·`text/*` 로만 맞으면 HTML 이라, 브라우저와 `Accept: */*` 를 보내는 쪽(curl, `fetch()` 기본값)은 지금처럼
// HTML 을 받는다.
import { hasMarkdown, markdownPath } from "./src/markdownCopies.ts";

interface Env {
  // 정적 자산(`dist/`). `html_handling` · `not_found_handling` · `_headers` 가 그대로 적용된다.
  ASSETS: { fetch(request: Request): Promise<Response> };
}

// 사이트가 사는 하위 경로 — `astro.config.mjs` 의 `base`.
const BASE = "/mdwire";

// `text/html` 에 적용되는 범위와 그 구체성. 더 구체적인 범위의 q 가 이긴다.
const HTML_RANGES = new Map([
  ["text/html", 2],
  ["text/*", 1],
  ["*/*", 0],
]);

function prefersMarkdown(accept: string | null): boolean {
  if (!accept) return false;
  let markdown = 0;
  // `text/html` 에 적용되는 범위가 없으면 HTML 의 q 는 0 이다.
  let html = { rank: -1, q: 0 };
  for (const item of accept.split(",")) {
    const [range, ...params] = item.split(";").map((s) => s.trim().toLowerCase());
    const qParam = params.find((p) => p.startsWith("q="));
    const q = qParam === undefined ? 1 : Number(qParam.slice(2));
    if (Number.isNaN(q)) continue;
    if (range === "text/markdown") markdown = Math.max(markdown, q);
    const rank = HTML_RANGES.get(range);
    if (rank === undefined) continue;
    if (rank > html.rank || (rank === html.rank && q > html.q)) html = { rank, q };
  }
  return markdown > 0 && markdown >= html.q;
}

// 정식 주소(끝 슬래시)인 페이지의 사본 주소. `/mdwire/` → `/mdwire/index.md`, `/mdwire/ko/docs/npm/` →
// `/mdwire/ko/docs/npm.md`. 사본이 없는 페이지와 정식 주소가 아닌 요청(`/mdwire/docs/npm` — 정적 자산이 끝 슬래시로
// 보낸다)은 null.
function copyOf(pathname: string): string | null {
  if (!pathname.startsWith(`${BASE}/`) || !pathname.endsWith("/")) return null;
  const path = pathname.slice(BASE.length).replace(/^\/|\/$/g, "");
  const rest = path.replace(/^ko(\/|$)/, "");
  return hasMarkdown(rest) ? `${BASE}${markdownPath(path)}` : null;
}

// 협상한 응답은 HTML 이든 마크다운이든 `Vary: Accept` 를 단다 — 브라우저 캐시가 둘을 섞지 않게.
function negotiated(response: Response, contentType?: string): Response {
  const out = new Response(response.body, response);
  out.headers.append("Vary", "Accept");
  if (contentType && out.status === 200) out.headers.set("Content-Type", contentType);
  return out;
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const copy = request.method === "GET" || request.method === "HEAD" ? copyOf(new URL(request.url).pathname) : null;
    if (!copy) return env.ASSETS.fetch(request);
    if (prefersMarkdown(request.headers.get("Accept"))) {
      const markdown = await env.ASSETS.fetch(new Request(new URL(copy, request.url), request));
      // 없는 페이지(`/mdwire/docs/nope/`)는 사본도 404 다 — 그때는 HTML 의 404 로 돌아간다. 304 는 사본의 ETag 로 다시
      // 물은 것이라 그대로 준다.
      if (markdown.status === 200 || markdown.status === 304) {
        // `_headers` 의 `/mdwire/*.md` 규칙도 사본 요청에 붙지만, 페이지 주소로 나가는 응답의 타입은 여기서 정한다.
        return negotiated(markdown, "text/markdown; charset=utf-8");
      }
    }
    return negotiated(await env.ASSETS.fetch(request));
  },
};
