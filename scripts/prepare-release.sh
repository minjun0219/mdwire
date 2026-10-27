#!/usr/bin/env bash
# 다음 판을 준비한다 — 버전 · Cargo.lock · CHANGELOG · README 설치 줄.
# 파일만 고치고 커밋·푸시는 하지 않는다(`.github/workflows/release-pr.yml` 이 한다).
# 로컬에서 그대로 돌려 볼 수 있다: `./scripts/prepare-release.sh && git diff`.
#
# 표준출력으로 다음 버전(`0.1.2` 꼴)을 한 줄 낸다. 마지막 태그 뒤로 낼 커밋이 없으면
# 아무것도 고치지 않고 빈 출력으로 끝난다.
#
# **버전은 패치를 올린다. 깨지는 변경이 있으면 0.x 라도 minor 를 올린다**(1.0 뒤로는 major).
# 깨지는 변경은 커밋 제목의 `!`(`refactor!: …`)나 본문의 `BREAKING CHANGE:` 꼬리말로 적는다
# (`AGENTS.md`). 0.x 에서 minor 를 올려야 cargo 의 `0.1` · npm 의 `^0.1.9` 같은 요구가 그 판을 받지
# 않는다 — 패치로 내면 하류가 버전을 고정하지 않은 한 말없이 깨진다. feat 만으로는 minor 를
# 올리지 않는다. 다른 버전을 원하면 main 의 `[workspace.package] version` 을 그 값으로 바꾼다.
# 그 값이 자동으로 고른 다음 판보다 크면 이 스크립트는 그쪽을 따른다.
set -euo pipefail
cd "$(dirname "$0")/.."

last=$(git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null || echo v0.0.0)
last=${last#v}

# 낼 것이 있는가 — **배포물이 바뀐 커밋만 센다.** 사이트·예제·문서·시험·CI 만 바꾼 커밋으로
# 판을 내면 내용이 똑같은 crates.io·npm 판과 Go 태그가 나간다(사이트 커밋 하나로 0.1.x 가
# 오르던 것). 배포물의 원본은 아래 경로뿐이다 — 코어·CLI·wasm 바인딩의 소스와 npm 패키징,
# 워크스페이스 매니페스트·잠금, Go 이식(시험 제외). README 는 패키지에 담기지만 문서라 판을
# 내지 않는다 — 다음 판에 같이 나간다. 하네스·벤치는 배포하지 않는다.
#
# `release:` 커밋은 버전만 바꾼 판이라 세지 않는다.
# (`git log | grep -q` 로 쓰면 grep 이 먼저 끝날 때 git 이 SIGPIPE 로 죽고, pipefail 이
# 그걸 "없음"으로 읽는다. 목록을 먼저 받아 둔다.)
module_paths=(
  crates/mdwire-core crates/mdwire-cli crates/mdwire-wasm
  Cargo.toml Cargo.lock scripts/build-npm.sh
  go
  ':(exclude)crates/*/tests/*' ':(exclude)go/*_test.go' ':(exclude)go/testdata/*'
)
if [ "$last" != 0.0.0 ]; then
  pending=$(git log --format=%s "v$last..HEAD" -- "${module_paths[@]}" | grep -v '^release' || true)
  [ -n "$pending" ] || exit 0
  # 깨지는 변경은 배포물 경로 밖(문서만 바꾼 커밋)에서는 나오지 않는다 — 같은 경로로 본다.
  breaking=$(git log --format='%s%n%b' "v$last..HEAD" -- "${module_paths[@]}" \
    | grep -E '^[a-z]+(\([^)]*\))?!:|^BREAKING[ -]CHANGE:' || true)
fi

current=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)
IFS=. read -r major minor patch <<<"$last"
if [ -z "${breaking:-}" ]; then
  next="$major.$minor.$((patch + 1))"
elif [ "$major" = 0 ]; then
  next="0.$((minor + 1)).0"
else
  next="$((major + 1)).0.0"
fi
if [ "$(printf '%s\n%s\n' "$next" "$current" | sort -V | tail -1)" = "$current" ]; then
  next=$current
fi

# 크레이트는 한 버전을 같이 쓴다(`version.workspace = true`). CLI 가 코어를 부르는
# 버전 요구도 같이 맞춘다 — 레지스트리에 올릴 때 이 값이 쓰인다.
perl -pi -e 's/^version = ".*"/version = "'"$next"'"/ if /^\[workspace\.package\]/../^\[(?!workspace\.package)/' Cargo.toml
perl -pi -e 's/(mdwire-core = \{ path = "\.\.\/mdwire-core", version = ")[^"]*/${1}'"$next"'/' crates/mdwire-cli/Cargo.toml
cargo update --workspace --quiet

# README 의 설치 줄은 이 판의 산출물을 가리킨다. 안 나간 버전을 가리키면 404 다. 머리의
# 상태 줄(`**Status: vX.**` · `**상태: vX.**`)도 같은 판을 적는다. 영어·한국어 두 벌을 같이 고친다.
perl -pi -e 's{releases/download/v[0-9.]+/mdwire-[0-9.]+\.tgz}{releases/download/v'"$next"'/mdwire-'"$next"'.tgz}g;
             s{releases/download/v[0-9.]+/mdwire-v[0-9.]+-}{releases/download/v'"$next"'/mdwire-v'"$next"'-}g;
             s{^\*\*(Status|상태): v[0-9.]+\.\*\*}{**$1: v'"$next"'.**}' README.md README.ko.md

# 변경 기록은 태그 전체에서 다시 만든다 — 손으로 고친 흔적이 남지 않는다.
git cliff --tag "v$next" --output CHANGELOG.md 2>/dev/null

echo "$next"
