#!/usr/bin/env bash
# 영어·한국어 README 가 같이 가는지 **모양**으로 본다. 문장은 번역이라 기계로 못 대조하지만,
# 한쪽에만 생긴 절·코드 블록·명령 줄·링크는 잡힌다 — 절 깊이의 차례, 코드 블록 수, 코드 줄 수,
# 링크 목록이 같아야 한다. 코드 블록 안의 `#` 줄은 셸 주석이라 절로 세지 않는다.
set -euo pipefail
cd "$(dirname "$0")/.."

shape() {
  awk '
    /^```/ { fence++; inside = !inside; next }
    inside { code++; next }
    /^#+ / { match($0, /^#+/); levels = levels RLENGTH " " }
    END { print "절 깊이: " levels; print "코드 블록: " fence / 2; print "코드 줄: " code }
  ' "$1"
  echo "링크:"
  grep -o 'https\?://[^ )>`"]*' "$1" | sort -u
}

if ! diff <(shape README.md) <(shape README.ko.md) > /dev/null; then
  echo "README.md 와 README.ko.md 의 모양이 다르다 — 한쪽만 고쳤다 (< 영어, > 한국어):" >&2
  diff <(shape README.md) <(shape README.ko.md) >&2 || true
  exit 1
fi
echo "README 두 벌이 같이 간다"
