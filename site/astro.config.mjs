import { execFileSync } from "node:child_process";
import { writeFile } from "node:fs/promises";
import mdx from "@astrojs/mdx";
import react from "@astrojs/react";
import { defineConfig } from "astro/config";
import wasm from "vite-plugin-wasm";

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

export default defineConfig({
  site: "https://mdwire.minjun.dev",
  integrations: [react(), mdx(), versionFile()],
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
