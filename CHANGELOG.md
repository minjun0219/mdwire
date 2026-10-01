# 변경 기록

커밋에서 만든다 — `git cliff`. 손으로 고치지 않는다.

## 0.1.10 — 2026-10-01

### 고친 것

- **core** — 슬랙에서 조사 앞 구두점 강조에 워드 조이너를 끼운다 ([#85](https://github.com/minjun0219/mdwire/pull/85))

### 문서

- 사이트와 README 의 어색한 영어 표현을 다듬는다 ([#86](https://github.com/minjun0219/mdwire/pull/86))

### 유지

- **release** — Linux arm64 · Windows 바이너리를 더하고 PR 에서 돌려 본다 ([#87](https://github.com/minjun0219/mdwire/pull/87))
- **release** — 그새 main 이 움직였으면 release PR 을 건드리지 않는다 ([#79](https://github.com/minjun0219/mdwire/pull/79))

### refactor

- 입력 표기 옵션을 걷어낸다 ([#84](https://github.com/minjun0219/mdwire/pull/84))
- 한국어 용어를 맞추고 CLI · 바인딩 오류 메시지를 다듬는다 ([#77](https://github.com/minjun0219/mdwire/pull/77))

## 0.1.9 — 2026-10-01

### 새로 할 수 있는 것

- **core** — 보고가 채널에 맞춰 바꾼 것도 센다 ([#66](https://github.com/minjun0219/mdwire/pull/66))
- **core** — 노션에는 표를 table 태그로 낸다 ([#63](https://github.com/minjun0219/mdwire/pull/63))
- **core** — 노션에서 강조를 줄마다 닫는다 ([#62](https://github.com/minjun0219/mdwire/pull/62))
- **core** — 노션 페이지 채널을 더한다 ([#61](https://github.com/minjun0219/mdwire/pull/61))

### 고친 것

- **core** — GitHub 에서 줄 첫머리 인라인 태그를 살린다 ([#65](https://github.com/minjun0219/mdwire/pull/65))
- **cli** — --이름=값 형식도 받는다 ([#64](https://github.com/minjun0219/mdwire/pull/64))

### 문서

- 한국어 문구를 다듬고 용어를 맞춘다 ([#76](https://github.com/minjun0219/mdwire/pull/76))
- **spec** — 불릿과 표를 다시 쓰는 이유를 적는다 ([#67](https://github.com/minjun0219/mdwire/pull/67))

### 유지

- **release** — 배포물이 바뀐 커밋으로만 판을 낸다 ([#73](https://github.com/minjun0219/mdwire/pull/73))

## 0.1.8 — 2026-09-30

### 새로 할 수 있는 것

- **core** — 스트리밍 미리보기를 기본으로 둔다 ([#58](https://github.com/minjun0219/mdwire/pull/58))
- **core** — 브라우저 채널 정책을 옵션으로 연다 ([#56](https://github.com/minjun0219/mdwire/pull/56))
- **npm** — React 컴포넌트와 구조 출력을 더한다 ([#55](https://github.com/minjun0219/mdwire/pull/55))
- **core** — 조각 한도를 호출자가 정한다 ([#54](https://github.com/minjun0219/mdwire/pull/54))
- **core** — 브라우저에 넣을 html 채널을 더한다 ([#53](https://github.com/minjun0219/mdwire/pull/53))
- **core** — GitHub 코멘트 채널을 더한다 ([#52](https://github.com/minjun0219/mdwire/pull/52))

### 고친 것

- **core** — 미리보기에서 코드 스팬을 인라인 층이 닫는다 ([#59](https://github.com/minjun0219/mdwire/pull/59))

### 문서

- **examples** — React 스트리밍 비교 예제를 둔다 ([#57](https://github.com/minjun0219/mdwire/pull/57))
- **agents** — 게이트를 CI 와 맞추고 저장소 지도를 둔다 ([#50](https://github.com/minjun0219/mdwire/pull/50))

## 0.1.7 — 2026-09-29

### 유지

- **readme** — 영어·한국어 README 가 같이 가는지 본다 ([#48](https://github.com/minjun0219/mdwire/pull/48))

## 0.1.6 — 2026-09-29

### 고친 것

- **release** — npm 11 을 전역 설치 대신 npx 로 쓴다 ([#46](https://github.com/minjun0219/mdwire/pull/46))

### 성능

- **core** — 붙든 `[`·`<!--` 를 조각마다 처음부터 다시 훑지 않는다 ([#45](https://github.com/minjun0219/mdwire/pull/45))

## 0.1.5 — 2026-09-29

### 문서

- **readme** — 레지스트리 설치 줄을 더한다 ([#43](https://github.com/minjun0219/mdwire/pull/43))

## 0.1.4 — 2026-09-29

### 새로 할 수 있는 것

- **npm** — 패키지 이름을 @minjun0219/mdwire 로 바꾼다 ([#41](https://github.com/minjun0219/mdwire/pull/41))

## 0.1.3 — 2026-09-29

### 유지

- **release** — crates.io 와 npm 에 Trusted Publishing 으로 올린다 ([#39](https://github.com/minjun0219/mdwire/pull/39))

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

