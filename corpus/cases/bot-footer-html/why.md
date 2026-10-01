LLM 답을 GitHub 코멘트로 올리는 봇의 꼬리말이다(실사용 보고, 2026-10-01). 코멘트 19개 중 16개가
답 끝에 `<sub>모델 · 토큰 · 비용</sub>` 을 달았는데, `github-markdown` 이 줄 첫머리 태그를 통째로
벗겨 작은 글씨가 보통 글씨가 됐다. GitHub 렌더 API 는 `<p><sub>…</sub></p>` 로 그린다 — 태그
뒤에 글이 오는 줄은 HTML 블록이 아니다. 접어 두는 `<details><summary>` 도 GitHub 이 그린다.

기대: GitHub 에서는 꼬리말 `<sub>` 와 `<details>`/`<summary>` 가 남는다. 태그를 못 그리는 채널은
전처럼 벗긴다. 같은 문서의 조사 앞 굵게(`**「…」**가`)는 GitHub 에서 `<strong>`, 범위 물결표는 `\~`.
