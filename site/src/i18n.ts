// 화면 틀(머리글·메뉴·꼬리말)의 두 벌. 본문은 페이지(MDX)가 언어마다 따로 갖는다.
export const locales = ["en", "ko"] as const;
export type Locale = (typeof locales)[number];

export const ui = {
  en: {
    tagline: "A bridge that carries LLM Markdown to chat channels without breaking it.",
    nav: { home: "Overview", why: "Why", demo: "Demo", docs: "Docs" },
    why: { index: "Why another one", cases: "Cases" },
    docs: { index: "Concepts", npm: "npm (JS · React)", rust: "Rust", go: "Go", cli: "CLI" },
    demo: { index: "All demos", channels: "Telegram vs Slack", streaming: "Streaming", repair: "Before and after repair" },
    next: "next release",
    switchTo: "한국어",
    license: "MIT License",
    pager: { prev: "Previous", next: "Next" },
    code: { copy: "Copy", copied: "Copied", anchor: "Link to this section" },
    home: {
      tryDemo: "Try the demo",
      readDocs: "Read the docs",
      showcase: {
        title: "One answer, five channels",
        lead: "A real bot answer, as the model wrote it — and what mdwire sends to each channel.",
        input: "Agent output",
        output: "Sent to",
        broken: ["bold next to a particle", "bold across a line break", "unclosed bold", "unclosed fence"],
        source: "Test case",
      },
      channels: { title: "Channels", limit: "limit", none: "no limit" },
    },
  },
  ko: {
    tagline: "LLM 마크다운을 깨뜨리지 않고 채널에 맞게 변환하는 브리지.",
    nav: { home: "소개", why: "배경", demo: "데모", docs: "문서" },
    why: { index: "왜 또 만들었나", cases: "사례" },
    docs: { index: "Concepts", npm: "npm (JS · React)", rust: "Rust", go: "Go", cli: "CLI" },
    demo: { index: "데모 목록", channels: "Telegram vs Slack", streaming: "Streaming", repair: "깨진 마크다운 고치기" },
    next: "다음 릴리스",
    switchTo: "English",
    license: "MIT 라이선스",
    pager: { prev: "이전", next: "다음" },
    code: { copy: "복사", copied: "복사함", anchor: "이 절의 링크" },
    home: {
      tryDemo: "데모 보기",
      readDocs: "문서 읽기",
      showcase: {
        title: "답 하나, 채널 다섯",
        lead: "모델이 쓴 그대로의 실제 봇 답변과, mdwire 가 채널마다 보내는 것입니다.",
        input: "에이전트 출력",
        output: "보내는 곳",
        broken: ["조사 옆 굵게", "줄을 넘는 굵게", "닫히지 않은 굵게", "닫히지 않은 코드펜스"],
        source: "테스트 케이스",
      },
      channels: { title: "채널", limit: "제한", none: "제한 없음" },
    },
  },
} as const satisfies Record<Locale, unknown>;

export function localeOf(value: string | undefined): Locale {
  return value === "ko" ? "ko" : "en";
}
