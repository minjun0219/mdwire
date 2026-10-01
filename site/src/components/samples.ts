// 데모의 예시 입력 — `corpus/cases/<id>/input.md` 를 그대로 옮겼다(코퍼스는 실제로 겪은 고장만 둔다).
// 사이트는 독립 패키지라 코퍼스를 빌드 때 읽지 않는다. 코퍼스 입력이 바뀌면 여기도 고친다.
import type { Locale } from "../i18n";

export interface Sample {
  /** 코퍼스 케이스 이름. */
  id: string;
  from: "markdown" | "slack-mrkdwn";
  note: Record<Locale, string>;
  input: string;
}

export const samples: Sample[] = [
  {
    id: "chat-bot-answer",
    from: "markdown",
    note: {
      en: "A real bot answer: bold next to a Korean particle, bold across a line break, unclosed bold, unclosed fence.",
      ko: "실제 봇 답변입니다. 조사 옆 굵게, 줄을 넘는 굵게, 닫히지 않은 굵게, 닫히지 않은 코드펜스가 들어 있습니다.",
    },
    input: "**상품 상세 화면**은 `web-app` 에 있어요.\n**첫 줄 설명이\n둘째 줄까지** 이어져요.\n\n**영향 범위: 세 화면\n예시:\n```ts\nconst a = 1;\n\n| 화면 | 경로 |\n|---|---|\n| 상품 상세 | /items/[id] |\n",
  },
  {
    id: "emphasis-across-linebreak",
    from: "markdown",
    note: {
      en: "Emphasis wrapped across lines. A regex converter pairs the stray ** with the next one and inverts the range.",
      ko: "줄을 넘는 강조입니다. 정규식 기반 변환기는 짝 없는 ** 를 뒤의 것과 짝지어 범위를 뒤집습니다.",
    },
    input: "공개 채널**이다. 글 내용이 아니라\n**신분 공개 + 시점의 조합**이 판단 대상\n",
  },
  {
    id: "cjk-adjacent-emphasis",
    from: "markdown",
    note: {
      en: "** right next to Korean text. Nothing is padded in.",
      ko: "한글 바로 옆에 붙은 ** 입니다. 사이에 공백을 끼워 넣지 않습니다.",
    },
    input: "배포는 **금요일**에 하지 않는다. `main` 브랜치에 **직접 커밋**하지도 않는다.\n",
  },
  {
    id: "unclosed-code-fence",
    from: "markdown",
    note: {
      en: "The agent stopped before closing the fence. It is closed right before output.",
      ko: "에이전트가 코드펜스를 닫지 않고 끝냈습니다. 출력 직전에 닫습니다.",
    },
    input: "로그는 이렇게 본다.\n\n```sh\ntail -f app.log | grep ERROR\n",
  },
  {
    id: "korean-table",
    from: "markdown",
    note: {
      en: "Telegram has no tables, so it goes out as a monospace block — columns aligned by display width, where Hangul takes two cells.",
      ko: "텔레그램에는 표가 없어서 고정폭 블록으로 나갑니다. 한글을 두 칸 너비로 계산해 열을 맞춥니다.",
    },
    input: "| 환경 | 재현 | 응답 시간 |\n|---|:---:|---:|\n| 스테이징 | 예 | 120ms |\n| 프로덕션 | 예 | 340ms |\n| 로컬 | 아니오 | 15ms |\n",
  },
  {
    id: "mrkdwn-bold-across-linebreak",
    from: "slack-mrkdwn",
    note: {
      en: "Legacy Slack mrkdwn (*bold*) across a line break, read with from: \"slack-mrkdwn\".",
      ko: "줄을 넘는 레거시 슬랙 mrkdwn(*굵게*)입니다. from: \"slack-mrkdwn\" 으로 읽습니다.",
    },
    input: "*첫 줄에서 열고 이어서\n둘째 줄에서 닫힘*입니다.\n\n앞말 *한 줄을 넘는\n굵게* 뒤에 말이 붙는다.\n",
  },
  {
    id: "table-cell-overflow",
    from: "markdown",
    note: {
      en: "A | inside a code span splits the table cell — and GFM silently drops the cell that no longer fits.",
      ko: "코드 스팬 안의 | 가 표 칸을 나눕니다. GFM 은 넘치는 칸을 아무 경고 없이 버립니다.",
    },
    input: "| 바이트 | 필드 | 뜻 | 비고 |\n| --- | --- | --- | --- |\n| `[3]` | `side` | 사이드 플래그 | 비트필드 |\n| `[4]` | `vol|wlv` | 볼륨/물수위 | 상위4비트=볼륨, 하위4비트=물수위 |\n",
  },
  {
    id: "unpaired-backtick-run",
    from: "markdown",
    note: {
      en: "A ``` with no partner in the middle of a line. Opened as a code span, it would swallow the bold after it.",
      ko: "줄 가운데에 있는 짝 없는 ``` 입니다. 코드 스팬으로 열면 뒤의 굵게까지 삼킵니다.",
    },
    input: "- Candidate: left_set = data[7] & 0x7F ``` 다음 발견 - **매트는 단일 연결 지원** - **bleak timeout=30s** 설정\n\n짝이 맞는 `코드 스팬` 과 **굵게** 는 그대로다.\n",
  },
];
