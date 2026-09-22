#!/usr/bin/env bash
# npm 패키지를 만든다. 산출물은 `pkg/`. 릴리스에 붙이는 tarball 은 `npm pack ./pkg`.
#
# **타깃이 둘이다.** wasm-pack 은 한 번에 한 타깃만 내는데, 번들러(vite·webpack)와
# 맨 Node(`node app.ts`)는 wasm 을 싣는 방법이 다르다 — 번들러 타깃은 번들러 없이
# 못 부른다. 실제 첫 소비자(슬랙 봇)가 Node 에서 바로 돌아서 둘 다 싣고, `exports` 의
# 조건으로 가른다: `node` 면 CommonJS 빌드, 아니면 ESM 번들러 빌드.
#
# **루트 package.json 은 여기서 직접 쓴다.** wasm-pack 은 `name` 을 crate 이름에서
# 가져오는데, crate 는 `mdwire-wasm` 이어야 하고(코어의 lib 이름이 `mdwire` 라 겹친다)
# npm 에 올릴 이름은 `mdwire` 다. 버전은 워크스페이스 `Cargo.toml` 이 정본이다.
set -euo pipefail

cd "$(dirname "$0")/.."
OUT=pkg

command -v wasm-pack >/dev/null || { echo "wasm-pack 이 없다: cargo install wasm-pack" >&2; exit 1; }

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[ -n "$VERSION" ] || { echo "Cargo.toml 에서 버전을 못 읽었다" >&2; exit 1; }

rm -rf "$OUT"
wasm-pack build crates/mdwire-wasm --release --target bundler --out-dir "../../$OUT/bundler" --out-name mdwire
wasm-pack build crates/mdwire-wasm --release --target nodejs  --out-dir "../../$OUT/node"    --out-name mdwire

# 하위 디렉토리의 package.json 은 모듈 종류만 말한다. Node 는 **가장 가까운** package.json
# 의 `type` 을 보므로, 루트가 ESM 이어도 node/ 는 CommonJS 로 읽힌다.
echo '{ "type": "module" }'   > "$OUT/bundler/package.json"
echo '{ "type": "commonjs" }' > "$OUT/node/package.json"
rm -f "$OUT"/*/.gitignore

cat > "$OUT/package.json" <<JSON
{
  "name": "mdwire",
  "version": "$VERSION",
  "description": "Streaming Markdown renderer for chat channels. Telegram HTML, Slack markdown_text, plain text.",
  "license": "MIT",
  "repository": { "type": "git", "url": "https://github.com/minjun0219/mdwire" },
  "homepage": "https://github.com/minjun0219/mdwire",
  "keywords": ["markdown", "telegram", "slack", "streaming", "llm", "wasm"],
  "type": "module",
  "main": "./node/mdwire.js",
  "types": "./bundler/mdwire.d.ts",
  "exports": {
    ".": {
      "types": "./bundler/mdwire.d.ts",
      "node": "./node/mdwire.js",
      "default": "./bundler/mdwire.js"
    }
  },
  "sideEffects": ["./bundler/mdwire.js"],
  "files": ["bundler", "node"]
}
JSON

echo
echo "만들었다: $OUT ($VERSION)"
find "$OUT" -type f | sort | while read -r f; do printf "  %8s  %s\n" "$(wc -c <"$f" | tr -d ' ')" "$f"; done
