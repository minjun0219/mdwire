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

# **tarball 로 설치한다.** 디렉터리를 넘기면 npm 은 심볼릭 링크만 걸어서, `files` 에서
# 빠진 파일(예: `.wasm`)이 있어도 여기서는 안 잡힌다. 릴리스에 붙는 것과 같은 tgz 를 쓴다.
npm pack --silent --pack-destination "$TMP" "$ROOT/pkg" >/dev/null
TGZ="$(ls "$TMP"/*.tgz)"

cd "$TMP"
npm init -y >/dev/null
# 첫 소비자와 같게 ESM 프로젝트로 둔다. 이게 없으면 `.ts` 자체가 CommonJS 로 읽힌다.
npm pkg set type=module >/dev/null
npm install --no-audit --no-fund "$TGZ" >/dev/null
# React 는 선택 peer 의존이다 — `./react` 를 쓰는 소비자처럼 같이 깐다.
npm install --no-audit --no-fund react react-dom @types/react >/dev/null
# `./lezer` 도 선택 peer 의존이다 — lezer 로 그리는 소비자(CodeMirror)처럼 같이 깐다. **peer 범위의 바닥 판으로
# 깐다** — 확장이 쓰는 `getDelimiterAt` 은 1.5.0 에서 생겼다. 최신판만 깔면 바닥이 깨져도 모른다. 최신판은 맨 끝에 한 번 더.
LEZER_MIN=$(node -p "require('./node_modules/@minjun0219/mdwire/package.json').peerDependencies['@lezer/markdown'].replace(/^[^0-9]*/, '')")
npm install --no-audit --no-fund "@lezer/markdown@$LEZER_MIN" >/dev/null
cp "$ROOT/scripts/smoke.mjs" "$ROOT/scripts/smoke.ts" "$ROOT/scripts/smoke-lezer.mjs" .

# 1. Node 에서 ESM 으로 import — `node` 조건이 CommonJS 빌드로 이어져야 한다.
node smoke.mjs
node smoke-lezer.mjs

# 2. Bun — 같은 파일을 그대로. Bun 은 `node` 조건을 골라 CommonJS 빌드를 읽고, wasm 은
#    `fs` 로 디스크에서 읽는다(2026-09-29 확인). 빌드 단계 없이 TypeScript ESM 으로 도는
#    소비자가 이 경로로 붙는다.
#    CI 에서는 건너뛰지 않는다 — 러너에 Bun 이 없어 조용히 건너뛰던 적이 있다.
if command -v bun >/dev/null; then
  bun smoke.mjs | sed 's/^/bun: /'
  bun smoke-lezer.mjs | sed 's/^/bun: /'
elif [ -n "${CI:-}" ]; then
  echo "CI 에 bun 이 없다 — ci.yml 에서 깔아야 한다" >&2
  exit 1
else
  echo "bun 이 없어 Bun 확인은 건너뛴다" >&2
fi

# 최신 @lezer/markdown 으로도 한 번 — 바닥 판과 최신판 사이에서 API 가 바뀌면 여기서 잡힌다.
npm install --no-audit --no-fund @lezer/markdown@latest >/dev/null
node smoke-lezer.mjs | sed 's/^/latest: /'

# 3. TypeScript — 첫 소비자의 설정(nodenext · verbatimModuleSyntax)으로 타입이 서는지.
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
elif [ -n "${CI:-}" ]; then
  # Bun 과 같다 — CI(와 그걸 흉내 내는 CI=1 로컬 게이트)에서는 건너뛰지 않는다.
  echo "CI 에 tsc 가 없다 — npm install -g typescript 로 깐다" >&2
  exit 1
else
  echo "tsc 가 없어 타입 확인은 건너뛴다" >&2
fi
