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
  },
  ko: {
    tagline: "LLM 마크다운을 깨뜨리지 않고 채널에 맞게 변환하는 브리지.",
    nav: { home: "소개", why: "왜 mdwire", demo: "데모", docs: "문서" },
    why: { index: "왜 또 만들었나", cases: "사례" },
    docs: { index: "Concepts", npm: "npm (JS · React)", rust: "Rust", go: "Go", cli: "CLI" },
    demo: { index: "데모 목록", channels: "Telegram vs Slack", streaming: "Streaming", repair: "깨진 마크다운 고치기" },
    next: "다음 릴리스",
    switchTo: "English",
    license: "MIT 라이선스",
  },
} as const satisfies Record<Locale, unknown>;

export function localeOf(value: string | undefined): Locale {
  return value === "ko" ? "ko" : "en";
}
