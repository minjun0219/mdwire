#!/usr/bin/env bash
# 인라인 시험의 기대 출력을 Rust 코어(mdwire CLI)에서 다시 뽑는다.
#
# 두 구현이 같은 답을 내는지가 곧 이식의 정의다. 이 파일들을 손으로 고치지 않는다 —
# 러스트 쪽 동작이 바뀌면 여기서 다시 뽑고 diff 를 본다.
#
#   cargo build --release -p mdwire-cli && go/testdata/regen.sh
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
bin=${MDWIRE:-"$here/../../target/release/mdwire"}
channels=(telegram-html slack-markdown plain)
# `<이름>-input.txt` 마다 `<이름>.<채널>.txt` 를 만든다. inline 은 한 문단(인라인 파서만),
# block 은 문서(완성본 파이프라인)다 — 둘 다 러스트 CLI 로 뽑는다.
for set in inline block; do
  for ch in "${channels[@]}"; do : > "$here/$set.$ch.txt"; done
  n=0
  emit() {
    local input=${1%$'\n'}   # 마지막 개행은 구분자 몫이다
    for ch in "${channels[@]}"; do
      if [ "$n" -gt 0 ]; then printf '\n----\n' >> "$here/$set.$ch.txt"; fi
      printf '%s' "$input" | "$bin" --channel "$ch" >> "$here/$set.$ch.txt"
    done
    n=$((n + 1))
  }
  # 입력은 `----` 줄로 나뉜다. 입력 하나에 줄이 여럿일 수 있다.
  buf=""
  while IFS= read -r line || [ -n "$line" ]; do
    if [ "$line" = "----" ]; then
      emit "$buf"; buf=""
    else
      buf+="$line"$'\n'
    fi
  done < "$here/$set-input.txt"
  emit "$buf"
  for ch in "${channels[@]}"; do echo >> "$here/$set.$ch.txt"; done
  echo "$set: $n 건"
done
