import { execFileSync } from "node:child_process";
import { writeFile } from "node:fs/promises";
import mdx from "@astrojs/mdx";
import react from "@astrojs/react";
import { defineConfig } from "astro/config";
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

export default defineConfig({
  site: "https://mdwire.minjun.dev",
  integrations: [react(), mdx(), versionFile(), agents()],
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
