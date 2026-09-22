#!/usr/bin/env bash
# npm 패키지를 만든다. 산출물은 `pkg/`.
#
# **타깃이 둘이다.** wasm-pack 은 한 번에 한 타깃만 내는데, 번들러(vite·webpack)와
# 맨 Node(`node app.ts`)는 wasm 을 싣는 방법이 다르다 — 번들러 타깃은 번들러 없이
# 못 부른다. 실제 첫 소비자(슬랙 봇)가 Node 에서 바로 돌아서 둘 다 싣고, `exports` 의
# 조건으로 가른다: `node` 면 CommonJS 빌드, 아니면 ESM 번들러 빌드.
#
# **이름을 고쳐 쓰는 이유.** wasm-pack 은 package.json 의 `name` 을 crate 이름에서
# 가져온다. crate 는 `mdwire-wasm` 이어야 하고(코어의 lib 이름이 `mdwire` 라 겹친다),
# npm 에 올릴 이름은 `mdwire` 다. 그래서 루트 package.json 은 여기서 직접 쓴다.
set -euo pipefail

cd "$(dirname "$0")/.."
OUT=pkg

command -v wasm-pack >/dev/null || { echo "wasm-pack 이 없다: cargo install wasm-pack" >&2; exit 1; }

rm -rf "$OUT"
wasm-pack build crates/mdwire-wasm --release --target bundler --out-dir "../../$OUT/bundler" --out-name mdwire
wasm-pack build crates/mdwire-wasm --release --target nodejs  --out-dir "../../$OUT/node"    --out-name mdwire

python3 - "$OUT" <<'PY'
import json, os, sys

out = sys.argv[1]
gen = json.load(open(f"{out}/bundler/package.json"))   # 버전은 crate 에서 온 것을 그대로 쓴다

# 하위 디렉토리의 package.json 은 모듈 종류만 말한다. Node 는 **가장 가까운** package.json
# 의 `type` 을 보므로, 루트가 ESM 이어도 node/ 는 CommonJS 로 읽힌다.
for sub, kind in (("bundler", "module"), ("node", "commonjs")):
    json.dump({"type": kind}, open(f"{out}/{sub}/package.json", "w"))
    gi = f"{out}/{sub}/.gitignore"
    if os.path.exists(gi):
        os.remove(gi)

pkg = {
    "name": "mdwire",
    "version": gen["version"],
    "description": "Streaming Markdown renderer for chat channels. Telegram HTML, Slack markdown_text, plain text.",
    "license": gen["license"],
    "repository": gen["repository"],
    "homepage": "https://github.com/minjun0219/mdwire",
    "keywords": ["markdown", "telegram", "slack", "streaming", "llm", "wasm"],
    "type": "module",
    "main": "./node/mdwire.js",
    "types": "./bundler/mdwire.d.ts",
    "exports": {
        ".": {
            "types": "./bundler/mdwire.d.ts",
            "node": "./node/mdwire.js",
            "default": "./bundler/mdwire.js",
        }
    },
    "sideEffects": ["./bundler/mdwire.js"],
    "files": ["bundler", "node"],
}
json.dump(pkg, open(f"{out}/package.json", "w"), indent=2, ensure_ascii=False)
open(f"{out}/package.json", "a").write("\n")
PY

echo
echo "만들었다: $OUT"
find "$OUT" -type f | sort | while read -r f; do printf "  %8s  %s\n" "$(wc -c <"$f" | tr -d ' ')" "$f"; done
