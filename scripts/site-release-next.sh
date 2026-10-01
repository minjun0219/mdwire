#!/usr/bin/env bash
# 릴리스에 들어간 기능의 `<Next />` 표시를 사이트 문서에서 걷는다.
#
#   scripts/site-release-next.sh v0.1.10
#
# **릴리스 태그에 들어간 줄만 걷는다.** 줄을 마지막으로 바꾼 커밋(`git blame`)이 그 태그의 조상이면
# 그 기능은 이번 판에 나갔다. 태그 뒤에 main 에만 들어간 기능의 표시는 남는다. 표시를 단 뒤에 그 줄을
# 다시 고친 경우는 남을 수 있다 — 그건 손으로 걷는다.
#
# 표시를 설명하는 문장(`marked <Next />`, `<Next /> 가 붙은`)은 걷지 않는다. 표시가 하나도 안 남은
# 파일은 `import Next` 줄도 뺀다.
set -euo pipefail
tag=${1:?릴리스 태그를 준다 (예 v0.1.10)}
git rev-parse --verify --quiet "$tag^{commit}" >/dev/null || { echo "태그가 없다: $tag" >&2; exit 1; }

removed=0
while IFS= read -r file; do
  lines=()
  while IFS=: read -r n _; do
    text=$(sed -n "${n}p" "$file")
    case "$text" in
      *'marked <Next />'* | *'<Next /> 가 붙은'*) continue ;;
    esac
    commit=$(git blame --porcelain -L "$n,$n" -- "$file" | head -1 | cut -d' ' -f1)
    if git merge-base --is-ancestor "$commit" "$tag" 2>/dev/null; then
      lines+=("$n")
    fi
  done < <(grep -n '<Next />' "$file")
  [ ${#lines[@]} -gt 0 ] || continue
  for n in "${lines[@]}"; do
    # 앞 공백과 함께 뗀다 — `…join them <Next />.` → `…join them.`
    perl -pi -e "s/ ?<Next \\/>//g if \$. == $n" "$file"
    removed=$((removed + 1))
  done
  if ! grep -q '<Next' "$file"; then
    perl -ni -e 'print unless /^import Next from /' "$file"
  fi
  echo "$file: ${#lines[@]}"
done < <(git grep -l '<Next />' -- site/src)

echo "걷은 표시: $removed"
