import { execFileSync } from "node:child_process";
import { rename, writeFile } from "node:fs/promises";
import mdx from "@astrojs/mdx";
import react from "@astrojs/react";
import { defineConfig, envField } from "astro/config";
import wasm from "vite-plugin-wasm";
import agents from "./integrations/agents.mjs";

// 서빙 중인 것이 올린 그것인가 — 빌드한 커밋을 `/_version.txt` 에 남긴다.
// Workers Builds 는 커밋을 WORKERS_CI_COMMIT_SHA 로 준다. 로컬 빌드는 git 에서 읽고, 커밋 안 된
// 변경이 있으면 `-dirty` 를 붙인다 — 그 산출물은 어느 커밋과도 같지 않다.
function versionFile() {
  const git = (...args) => {
    try {
      return execFileSync("git", args, { encoding: "utf8" }).trim();
    } catch {
      return "";
    }
  };
  return {
    name: "mdwire-version-file",
    hooks: {
      "astro:build:done": async ({ dir }) => {
        let sha = process.env.WORKERS_CI_COMMIT_SHA;
        if (!sha) {
          sha = git("rev-parse", "HEAD") || "unknown";
          if (git("status", "--porcelain")) sha += "-dirty";
        }
        await writeFile(new URL("_version.txt", dir), `${sha}\n`);
      },
    },
  };
}

// 고정폭 블록 속 전각 글자(한글 · 한자 · 전각 기호)를 정확히 영문 두 칸(`2ch`) 너비로 감싼다. 글꼴마다 한글
// 폭이 영문의 두 배가 아니어서, 표시 폭으로 맞춘 표가 화면에서 어긋나기 때문이다. ```text cells 처럼 meta 에
// `cells` 를 단 블록에만 쓴다 — 다른 코드는 글꼴 그대로 둔다.
const WIDE = /[\u1100-\u115F\u2E80-\u303E\u3041-\u33FF\u3400-\u4DBF\u4E00-\u9FFF\uA960-\uA97F\uAC00-\uD7A3\uF900-\uFAFF\uFE30-\uFE4F\uFF00-\uFF60\uFFE0-\uFFE6]/u;
function wideCells() {
  const split = (node) => {
    if (!node.children) return;
    node.children = node.children.flatMap((child) => {
      if (child.type !== "text") {
        split(child);
        return [child];
      }
      const out = [];
      let run = "";
      for (const ch of child.value) {
        if (!WIDE.test(ch)) {
          run += ch;
          continue;
        }
        if (run) out.push({ type: "text", value: run });
        run = "";
        out.push({ type: "element", tagName: "span", properties: { className: ["wide"] }, children: [{ type: "text", value: ch }] });
      }
      if (run) out.push({ type: "text", value: run });
      return out;
    });
  };
  return {
    name: "mdwire-wide-cells",
    code(node) {
      if (this.options.meta?.__raw?.split(/\s+/).includes("cells")) split(node);
    },
  };
}

// Workers Static Assets 는 `_headers` 를 자산 디렉터리의 루트에서만 읽는다. 페이지는 `dist/mdwire/` 아래에 나오니(`base`),
// `public/` 에서 함께 복사된 `_headers` 를 한 단 위(`dist/`)로 올린다.
function assetsRoot() {
  return {
    name: "mdwire-assets-root",
    hooks: {
      "astro:build:done": async ({ dir }) => {
        await rename(new URL("_headers", dir), new URL("../_headers", dir));
      },
    },
  };
}

export default defineConfig({
  // `minjun.kim` 의 `/mdwire/` 아래에 산다 — 워커는 `minjun.kim/mdwire*` 라우트로 요청을 받고, 정적 자산은 요청 경로
  // 그대로 찾으므로 산출물도 `dist/mdwire/` 에 둔다(`wrangler.jsonc` 의 자산 디렉터리는 `dist/`). Astro 는 자기가 만드는
  // 링크에만 base 를 붙이므로 MDX 본문의 사이트 안 링크는 `/mdwire/docs/` 처럼 base 를 적어서 쓴다.
  site: "https://minjun.kim",
  base: "/mdwire",
  outDir: "./dist/mdwire",
  integrations: [react(), mdx(), versionFile(), agents(), assetsRoot()],
  // 방문 집계(PostHog)의 빌드 변수 — Workers Builds 의 프로덕션 빌드 변수에만 넣는다(`README.md` 배포 절). 브라우저로 나가는
  // 값이라 Astro 의 공개 접두사 `PUBLIC_` 을 단다. 둘 다 없을 수 있다 — 없는 빌드(로컬 · 프리뷰 · 포크)는 `src/analytics.ts` 가
  // 초기화하지 않고 SDK 코드도 담지 않는다. 호스트는 기본값을 두지 않는다 — posthog-js 자체 기본값(`https://us.i.posthog.com`)으로
  // 조용히 프록시를 우회하는 걸 막는다.
  env: {
    schema: {
      PUBLIC_POSTHOG_KEY: envField.string({ context: "client", access: "public", optional: true }),
      PUBLIC_POSTHOG_HOST: envField.string({ context: "client", access: "public", optional: true, url: true }),
    },
  },
  markdown: { shikiConfig: { transformers: [wideCells()] } },
  i18n: {
    locales: ["en", "ko"],
    defaultLocale: "en",
    // 영어는 `/`, 한국어는 `/ko/` — README.md 가 영어, README.ko.md 가 한국어인 것과 같다.
    routing: { prefixDefaultLocale: false },
  },
  // mdwire 의 번들러 빌드는 `.wasm` 을 ESM 으로 import 한다(wasm-pack bundler 타깃).
  // examples/react-streaming 과 같은 두 줄 — vite-plugin-wasm, 그리고 그 import 가 쓰는
  // top-level await 가 되도록 목표를 esnext 로.
  vite: {
    plugins: [wasm()],
    build: { target: "esnext" },
  },
});
