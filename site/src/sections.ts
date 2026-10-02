// 배경 · 문서 · 데모 묶음의 페이지 차례. 사이드바(`components/SectionNav.astro`)와 본문 아래의 이전 · 다음
// 링크(`components/Pager.astro`)가 같은 차례를 쓴다.
import { ui, type Locale } from "./i18n";

export type Section = "why" | "docs" | "demo";

export function sectionItems(section: Section, lang: Locale): { page: string; label: string }[] {
  const t = ui[lang];
  return {
    why: [
      { page: "why", label: t.why.index },
      { page: "why/cases", label: t.why.cases },
    ],
    docs: [
      { page: "docs", label: t.docs.index },
      { page: "docs/npm", label: t.docs.npm },
      { page: "docs/rust", label: t.docs.rust },
      { page: "docs/go", label: t.docs.go },
      { page: "docs/cli", label: t.docs.cli },
      { page: "docs/emphasis", label: t.docs.emphasis },
    ],
    demo: [
      { page: "demo", label: t.demo.index },
      { page: "demo/channels", label: t.demo.channels },
      { page: "demo/streaming", label: t.demo.streaming },
      { page: "demo/repair", label: t.demo.repair },
    ],
  }[section];
}
