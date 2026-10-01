// 검색 엔진과 에이전트가 읽을 파일을 빌드 끝에 만든다. 손으로 쓰는 `public/llms.txt` 말고 나머지:
//
// - `sitemap.xml` — 빌드된 페이지 전부, 영어·한국어 짝을 hreflang 으로 묶어서.
// - 문서의 마크다운 사본 — `/docs/npm/` 은 `/docs/npm.md`. llmstxt.org 의 관례대로 HTML 을 긁지
//   않고 원문을 읽게 한다. MDX 원본에서 import 를 걷어 내고 컴포넌트를 글로 바꾼 것이다.
// - `llms-full.txt` — 영어 문서 사본을 한 파일로. 한 번 받아서 컨텍스트에 넣는 쪽.
//
// 사본이 있는 페이지의 규칙은 `src/markdownCopies.ts` — 레이아웃도 같은 규칙으로 `rel="alternate"` 를 단다.
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { ui } from "../src/i18n.ts";
import { hasMarkdown, markdownPath } from "../src/markdownCopies.ts";

// llms-full.txt 에 싣는 차례 — 개요, 배경(왜 또 만들었나 · 사례), 개념, 언어별 API 순.
const fullOrder = ["", "why", "why/cases", "docs", "docs/npm", "docs/rust", "docs/go", "docs/cli"];

// `src/pages` 안의 원본 위치. 묶음의 첫 페이지(`docs`, `why`)는 `docs/index.mdx`, 개요는 `index.mdx`.
const sourceOf = (lang, rest) => {
  const base = lang === "en" ? "" : `${lang}/`;
  const file = rest === "" ? "index" : rest === "docs" || rest === "why" ? `${rest}/index` : rest;
  return `src/pages/${base}${file}.mdx`;
};

function toMarkdown(source, { lang, site, url }) {
  const [, front, body] = source.match(/^---\n([\s\S]*?)\n---\n([\s\S]*)$/);
  const description = front.match(/^description:\s*"?(.*?)"?$/m)?.[1];
  let inFence = false;
  const lines = [];
  for (const line of body.split("\n")) {
    if (line.startsWith("```")) inFence = !inFence;
    if (!inFence) {
      if (/^import\s/.test(line)) continue;
      // 배지 줄은 이미지 링크뿐이라 사본에서 뺀다 — 패키지 주소는 본문에 있다.
      if (/^<Badges\b.*\/>$/.test(line)) continue;
      lines.push(
        line
          .replaceAll(/<Next\s*\/>/g, `(${ui[lang].next})`)
          // 사이트 안 링크는 사본 밖에서도 닿게 절대 주소로.
          .replaceAll(/\]\((\/[^)]*)\)/g, (_, path) => `](${new URL(path, site)})`),
      );
    } else {
      lines.push(line);
    }
  }
  let text = lines.join("\n").replace(/\n{3,}/g, "\n\n").trim();
  // 첫 헤딩 아래에 요약과 HTML 주소를 단다 — 사본만 받은 쪽도 어디서 왔는지 안다.
  const meta = [description && `> ${description}`, `HTML: ${url}`].filter(Boolean).join("\n\n");
  text = text.replace(/^(# .*)$/m, `$1\n\n${meta}`);
  return `${text}\n`;
}

const escapeXml = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;");

export default function agents() {
  let site;
  return {
    name: "mdwire-agent-files",
    hooks: {
      "astro:config:done": ({ config }) => {
        site = config.site;
      },
      "astro:build:done": async ({ dir, pages }) => {
        // 언어와 무관한 경로 → 그 페이지가 있는 언어들.
        const byRest = new Map();
        for (const { pathname } of pages) {
          const path = pathname.replace(/\/$/, "");
          if (path === "404") continue;
          const lang = path === "ko" || path.startsWith("ko/") ? "ko" : "en";
          const rest = path.replace(/^ko(\/|$)/, "");
          if (!byRest.has(rest)) byRest.set(rest, new Set());
          byRest.get(rest).add(lang);
        }
        const pageUrl = (lang, rest) => {
          const path = [lang === "en" ? "" : lang, rest].filter(Boolean).join("/");
          return new URL(path ? `/${path}/` : "/", site).href;
        };

        const urls = [];
        for (const [rest, langs] of [...byRest].sort(([a], [b]) => a.localeCompare(b))) {
          const links = [...langs].map(
            (l) => `    <xhtml:link rel="alternate" hreflang="${l}" href="${escapeXml(pageUrl(l, rest))}"/>`,
          );
          if (langs.has("en")) {
            links.push(`    <xhtml:link rel="alternate" hreflang="x-default" href="${escapeXml(pageUrl("en", rest))}"/>`);
          }
          for (const lang of langs) {
            urls.push(`  <url>\n    <loc>${escapeXml(pageUrl(lang, rest))}</loc>\n${links.join("\n")}\n  </url>`);
          }
        }
        await writeFile(
          new URL("sitemap.xml", dir),
          `<?xml version="1.0" encoding="UTF-8"?>\n` +
            `<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">\n` +
            `${urls.join("\n")}\n</urlset>\n`,
        );

        const full = new Map();
        for (const [rest, langs] of byRest) {
          if (!hasMarkdown(rest)) continue;
          for (const lang of langs) {
            const source = await readFile(new URL(`../${sourceOf(lang, rest)}`, import.meta.url), "utf8");
            const md = toMarkdown(source, { lang, site, url: pageUrl(lang, rest) });
            const path = [lang === "en" ? "" : lang, rest].filter(Boolean).join("/");
            const out = new URL(`.${markdownPath(path)}`, dir);
            await mkdir(new URL(".", out), { recursive: true });
            await writeFile(out, md);
            if (lang === "en") full.set(rest, md);
          }
        }
        const head =
          "# mdwire — full documentation\n\n" +
          "> Every page of the English docs in one file. The index with links is " +
          `${new URL("/llms.txt", site)}; each page also exists alone as Markdown (e.g. ` +
          `${new URL("/docs/npm.md", site)}).\n`;
        const pagesText = fullOrder.filter((r) => full.has(r)).map((r) => full.get(r).replace(/^# /, "## "));
        // 사본 안의 헤딩을 한 단씩 내려 한 문서의 절이 되게 한다.
        const body = pagesText.map((md) => demote(md)).join("\n");
        await writeFile(new URL("llms-full.txt", dir), `${head}\n${body}`);
      },
    },
  };
}

// 첫 줄(이미 `##`)을 뺀 헤딩을 한 단 내린다. 코드 블록 안의 `#` 은 건드리지 않는다.
function demote(md) {
  let inFence = false;
  return md
    .split("\n")
    .map((line, i) => {
      if (line.startsWith("```")) inFence = !inFence;
      return !inFence && i > 0 && /^#{2,5} /.test(line) ? `#${line}` : line;
    })
    .join("\n");
}
