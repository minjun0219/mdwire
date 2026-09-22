#!/usr/bin/env bash
# 만든 npm 패키지를 **설치해서** 불러 본다.
#
# `pkg/` 안의 파일을 직접 import 하면 `exports` 맵을 안 거친다 — 실제 소비자가 받는
# 해석(`node` 조건 → CommonJS, 타입 → `.d.ts`)을 그대로 밟으려면 진짜로 설치해야 한다.
# 빈 프로젝트를 임시로 만들어 거기서 돈다.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cd "$TMP"
npm init -y >/dev/null
# 첫 소비자와 같게 ESM 프로젝트로 둔다. 이게 없으면 `.ts` 자체가 CommonJS 로 읽힌다.
npm pkg set type=module >/dev/null
npm install --no-audit --no-fund "$ROOT/pkg" >/dev/null
cp "$ROOT/scripts/smoke.mjs" "$ROOT/scripts/smoke.ts" .

# 1. Node 에서 ESM 으로 import — `node` 조건이 CommonJS 빌드로 이어져야 한다.
node smoke.mjs

# 2. TypeScript — 첫 소비자의 설정(nodenext · verbatimModuleSyntax)으로 타입이 서는지.
#    `types` 를 비워 둔다. 빈 프로젝트라 @types/node 가 없고, 여기서 보는 것은 우리
#    `.d.ts` 가 그 설정에서 서느냐 하나뿐이다.
if command -v tsc >/dev/null; then
  cat > tsconfig.json <<'JSON'
{
  "compilerOptions": {
    "strict": true,
    "module": "nodenext",
    "moduleResolution": "nodenext",
    "verbatimModuleSyntax": true,
    "skipLibCheck": true,
    "types": [],
    "noEmit": true
  },
  "files": ["smoke.ts"]
}
JSON
  tsc -p . && echo "타입 통과 (nodenext · verbatimModuleSyntax)"
else
  echo "tsc 가 없어 타입 확인은 건너뛴다" >&2
fi
