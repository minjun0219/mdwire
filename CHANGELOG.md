# 변경 기록

커밋에서 만든다 — `git cliff`. 손으로 고치지 않는다.

## 0.1.2 — 2026-09-29

### 새로 할 수 있는 것

- **cli** — --report 를 여러 문서에 한 번에 돌린다 ([#37](https://github.com/minjun0219/mdwire/pull/37))

### 문서

- **readme** — 한국어 README 를 두고 v0.1.1 에 맞춘다 ([#30](https://github.com/minjun0219/mdwire/pull/30))
- **spec** — 채널 한도가 무엇을 세는지 적는다 ([#35](https://github.com/minjun0219/mdwire/pull/35))

### 테스트

- **corpus** — 줄을 넘는 mrkdwn 홑별표를 코퍼스로 고정한다 ([#36](https://github.com/minjun0219/mdwire/pull/36))

### 유지

- **npm** — 스모크에서 Bun 확인이 실제로 돌게 한다 ([#31](https://github.com/minjun0219/mdwire/pull/31))

## 0.1.1 — 2026-09-29

### 새로 할 수 있는 것

- append-only 계약 · 입력 방언 · 정규화 리포트 ([#27](https://github.com/minjun0219/mdwire/pull/27))
- **go** — Go 이식 — 코퍼스와 러스트 대조를 통과한다 ([#25](https://github.com/minjun0219/mdwire/pull/25))
- **wasm** — npm 패키지가 맨 Node 에서도 돌고 릴리스에 산출물이 붙는다 ([#20](https://github.com/minjun0219/mdwire/pull/20))

### 고친 것

- **core** — 추측으로 연 마커를 닫지 않는다 ([#23](https://github.com/minjun0219/mdwire/pull/23))
- **core** — 슬랙·텔레그램 실측에 맞춰 범위를 다시 잡는다 ([#22](https://github.com/minjun0219/mdwire/pull/22))

### 문서

- **design** — 다른 변환기를 같은 척도로 나란히 잰다 ([#26](https://github.com/minjun0219/mdwire/pull/26))

### 테스트

- **harness** — 무작위 입력 퍼즈로 불변식을 지킨다 ([#24](https://github.com/minjun0219/mdwire/pull/24))

### 유지

- **release** — 버전 올리기를 release PR 로 자동화한다 ([#28](https://github.com/minjun0219/mdwire/pull/28))

## 0.1.0 — 2026-09-22

### 새로 할 수 있는 것

- **wasm** — npm 패키지를 만들고 CI 가 실제로 불러 본다 ([#18](https://github.com/minjun0219/mdwire/pull/18))
- **harness** — 내용이 사라졌는지 재는 규칙 ([#7](https://github.com/minjun0219/mdwire/pull/7))
- **core** — 스트리밍 중간 스냅샷을 보낼 수 있게 close_open
- **wasm** — 브라우저·npm 바인딩
- **cli** — 인자 파싱과 스트리밍 모드
- **core** — 파서·채널 렌더러·안전 분할·스트리밍
- mdwire 뼈대 — 스트리밍 마크다운 채널 렌더러

### 고친 것

- **core** — 백틱을 담은 코드 스팬을 제대로 감싼다 ([#15](https://github.com/minjun0219/mdwire/pull/15))
- **core** — 역슬래시 탈출을 읽는다 ([#14](https://github.com/minjun0219/mdwire/pull/14))
- **core** — 표에서 넘치는 칸을 버리지 않는다 ([#13](https://github.com/minjun0219/mdwire/pull/13))
- **review** — 조각 경계에서 갈라진 범위를 좁아졌다고 세지 않는다 ([#12](https://github.com/minjun0219/mdwire/pull/12))
- **core** — 짝 없는 백틱 런은 글자로 되돌린다 ([#11](https://github.com/minjun0219/mdwire/pull/11))
- **core** — 마커 뒤의 탭도 공백으로 친다 ([#10](https://github.com/minjun0219/mdwire/pull/10))
- **core** — 한도보다 긴 주소는 링크로 내지 않는다 ([#9](https://github.com/minjun0219/mdwire/pull/9))
- **core** — 태그 한가운데서 조각을 끊지 않는다 ([#8](https://github.com/minjun0219/mdwire/pull/8))
- **core** — CJK 판정을 표시 폭과 분리한다 ([#6](https://github.com/minjun0219/mdwire/pull/6))
- **core** — 코드 스팬이 백틱 런 안쪽에서 잘못 닫히던 것 ([#5](https://github.com/minjun0219/mdwire/pull/5))
- **harness** — void 요소를 안 닫혔다고 세던 것 ([#3](https://github.com/minjun0219/mdwire/pull/3))
- 실제 문서 2281개를 훑어 나온 고장들 ([#2](https://github.com/minjun0219/mdwire/pull/2))
- **core** — 슬랙 취소선을 `~~` 로 낸다
- **harness** — 채점기가 정상 출력을 고장으로 신고하던 다섯
- **core** — 실제 문서 229개를 돌려 잡은 셋
- **core** — 구두점 뒤에 오는 닫는 마커가 안 닫히던 것
- **core** — CRLF 잔재와 세 개짜리 마커 런

### 성능

- **bench** — 디렉토리를 받아 실제 문서로 재는 모드
- **bench** — 스트리밍 서명 후보를 재는 계측 리그

### 문서

- **spec** — frontmatter 를 떼지 않는 이유를 적는다 ([#17](https://github.com/minjun0219/mdwire/pull/17))
- **spec** — 인용 안에서 블록이 다시 열리지 않는 것을 적는다 ([#16](https://github.com/minjun0219/mdwire/pull/16))
- **design** — CJK 정책 "제각각"에 수치를 붙인다 ([#4](https://github.com/minjun0219/mdwire/pull/4))
- **spec** — 처리량을 절대값 대신 상대비로 적는다
- v0.1 확정안 — 채널 셋, 정규화는 복구+정돈까지, 표는 고정폭

### 테스트

- **harness** — 코퍼스 실행기와 구현 중립 채점기

### 유지

- PR 에서 게이트를 자동으로 돌린다 ([#1](https://github.com/minjun0219/mdwire/pull/1))

