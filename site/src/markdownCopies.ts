// 마크다운 사본(`/docs/npm.md` …)이 있는 페이지의 규칙. 사본을 만드는 쪽(`integrations/agents.mjs`)과
// `rel="alternate"` 를 다는 쪽(`layouts/Doc.astro`)이 같이 쓴다.

// 언어와 무관한 경로(`""`, `docs`, `docs/npm` …) — 개요와 문서만. 데모는 브라우저에서 돌아야 뜻이 있다.
export const hasMarkdown = (rest: string) => rest === "" || rest === "docs" || rest.startsWith("docs/");

// 페이지 경로(`ko/docs/npm`) → 사본 주소. 루트만 이름이 없어 `index.md`.
export const markdownPath = (path: string) => (path === "" ? "/index.md" : `/${path}.md`);
