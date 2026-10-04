// 사이트는 `minjun.kim` 의 `/mdwire/` 아래에 산다(`astro.config.mjs` 의 `base`). Astro 의 `getRelativeLocaleUrl` 은 base 를
// 알고 붙이지만, 손으로 쓴 절대 경로(`/og.png`)와 `Astro.url.pathname` 에서 페이지 경로를 읽는 곳은 여기 도우미를 거친다.
// MDX 본문의 사이트 안 링크는 `](/mdwire/docs/)` 처럼 base 를 적어서 쓴다(`site/README.md`).
const base = import.meta.env.BASE_URL.replace(/\/$/, "");

/** `/og.png` → `/mdwire/og.png`. 인자는 `/` 로 시작한다. */
export const withBase = (path: string) => `${base}${path}`;

/** `/mdwire/ko/docs/` → `/ko/docs/`. base 없이 들어와도 그대로 돌려준다. */
export const stripBase = (pathname: string) =>
  base && (pathname === base || pathname.startsWith(`${base}/`)) ? pathname.slice(base.length) || "/" : pathname;
