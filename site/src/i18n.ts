// 화면 틀(머리글·메뉴·꼬리말)의 두 벌. 본문은 페이지(MDX)가 언어마다 따로 갖는다.
export const locales = ["en", "ko"] as const;
export type Locale = (typeof locales)[number];

export const ui = {
  en: {
    tagline: "A bridge that carries LLM Markdown to chat channels without breaking it.",
    nav: { home: "Overview", demo: "Demo", streaming: "Streaming" },
    switchTo: "한국어",
    source: "Source",
    footer: "MIT licensed.",
  },
  ko: {
    tagline: "LLM 마크다운을 채널에 맞게 깨지지 않게 옮기는 브릿지.",
    nav: { home: "개요", demo: "데모", streaming: "스트리밍" },
    switchTo: "English",
    source: "소스",
    footer: "MIT 라이선스.",
  },
} as const satisfies Record<Locale, unknown>;

export function localeOf(value: string | undefined): Locale {
  return value === "ko" ? "ko" : "en";
}
