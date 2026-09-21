#!/usr/bin/env bash
# npm 패키지를 만든다. 산출물은 `pkg/`.
#
# **이름을 고쳐 쓰는 이유.** wasm-pack 은 package.json 의 `name` 을 crate 이름에서
# 가져온다. crate 는 `mdwire-wasm` 이어야 하고(코어의 lib 이름이 `mdwire` 라 겹친다),
# npm 에 올릴 이름은 `mdwire` 다. 그래서 만든 뒤 한 번 고친다.
set -euo pipefail

cd "$(dirname "$0")/.."
OUT=pkg

command -v wasm-pack >/dev/null || { echo "wasm-pack 이 없다: cargo install wasm-pack" >&2; exit 1; }

rm -rf "$OUT"
wasm-pack build crates/mdwire-wasm --release --target bundler --out-dir "../../$OUT" --out-name mdwire

python3 - "$OUT/package.json" <<'PY'
import json, sys

path = sys.argv[1]
pkg = json.load(open(path))
pkg["name"] = "mdwire"
pkg["description"] = "Streaming Markdown renderer for chat channels. Telegram HTML, Slack markdown_text, plain text."
pkg["keywords"] = ["markdown", "telegram", "slack", "streaming", "llm", "wasm"]
pkg["homepage"] = "https://github.com/minjun0219/mdwire"
json.dump(pkg, open(path, "w"), indent=2, ensure_ascii=False)
open(path, "a").write("\n")
PY

echo
echo "만들었다: $OUT"
ls -la "$OUT" | awk 'NR>1 {printf "  %8s  %s\n", $5, $9}'
