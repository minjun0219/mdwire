// 같은 입력을 텔레그램 HTML 과 슬랙 markdown_text 로 옮겨 나란히 본다 — 채널이 실제로 받는 글자 그대로.
// 변환은 브라우저에서 npm `@minjun0219/mdwire` 의 wasm 이 한다. 서버는 없다.
import { useMemo, useState } from "react";
import { limit, renderWithReport } from "@minjun0219/mdwire";
import type { Locale } from "../i18n";
import { samples } from "./samples";
import styles from "./ChannelCompare.module.css";

const channels = [
  { id: "telegram-html", name: "Telegram HTML", via: 'parse_mode: "HTML"' },
  { id: "slack-markdown", name: "Slack markdown_text", via: "chat.postMessage · markdown_text" },
] as const;

// 정규화 보고의 열 가지 수 — 앞 넷은 고친 것, 뒤 여섯은 채널에 맞춰 바꾼 것.
const keys = [
  "closedEmphasis",
  "closedFence",
  "revertedCodeSpan",
  "droppedMarker",
  "escapedChar",
  "tagEmphasis",
  "strippedHtml",
  "rewrittenBullet",
  "rewrittenTable",
  "convertedMarker",
] as const;
type Repairs = Record<(typeof keys)[number], number>;

type Output = { parts: string[]; repairs: Repairs; limit: number } | { error: string };

const text = {
  en: {
    sample: "Example from the test cases",
    input: "Agent output (Markdown)",
    parts: (n: number, max: number) => `${n} ${n === 1 ? "part" : "parts"} · limit ${max.toLocaleString("en")}`,
    noRepairs: "Nothing repaired or rewritten",
    repairs: {
      closedEmphasis: "closed emphasis",
      closedFence: "closed code fence",
      revertedCodeSpan: "backtick run kept as text",
      droppedMarker: "stray ** dropped",
      escapedChar: "character escaped",
      tagEmphasis: "emphasis rewritten where markers fail",
      strippedHtml: "HTML stripped",
      rewrittenBullet: "bullet rewritten",
      rewrittenTable: "table rewritten",
      convertedMarker: "marker converted",
    },
    empty: "(empty)",
  },
  ko: {
    sample: "테스트 케이스 예시",
    input: "에이전트 출력 (마크다운)",
    parts: (n: number, max: number) => `조각 ${n}개 · 길이 제한 ${max.toLocaleString("ko")}`,
    noRepairs: "고치거나 바꾼 것 없음",
    repairs: {
      closedEmphasis: "닫아 준 강조",
      closedFence: "닫아 준 코드펜스",
      revertedCodeSpan: "텍스트로 되돌린 백틱",
      droppedMarker: "짝이 없어 버린 **",
      escapedChar: "이스케이프한 글자",
      tagEmphasis: "마커로 읽히지 않는 강조",
      strippedHtml: "제거한 HTML",
      rewrittenBullet: "바꿔 쓴 목록 기호",
      rewrittenTable: "다시 쓴 표",
      convertedMarker: "바꿔 쓴 마커",
    },
    empty: "(비어 있음)",
  },
} as const;

function convert(input: string, channel: string): Output {
  try {
    // wasm 쪽 객체라 읽고 나서 바로 놓는다.
    const out = renderWithReport(input, channel);
    const r = out.repairs;
    const repairs = Object.fromEntries(keys.map((k) => [k, r[k]])) as Repairs;
    const parts = out.parts;
    r.free();
    out.free();
    return { parts, repairs, limit: limit(channel) };
  } catch (e) {
    return { error: e instanceof Error ? e.message : String(e) };
  }
}

export default function ChannelCompare({ lang }: { lang: Locale }) {
  const t = text[lang];
  const [sampleId, setSampleId] = useState(samples[0].id);
  const [input, setInput] = useState(samples[0].input);
  const sample = samples.find((s) => s.id === sampleId);

  const outputs = useMemo(() => channels.map((c) => ({ ...c, out: convert(input, c.id) })), [input]);

  function pick(id: string) {
    const s = samples.find((x) => x.id === id);
    if (!s) return;
    setSampleId(id);
    setInput(s.input);
  }

  return (
    <div className={styles.root}>
      <div className={styles.controls}>
        <label>
          <span>{t.sample}</span>
          <select value={sampleId} onChange={(e) => pick(e.target.value)}>
            {samples.map((s) => (
              <option key={s.id} value={s.id}>
                {s.id}
              </option>
            ))}
          </select>
        </label>
      </div>
      {sample && sample.input === input && <p className={styles.note}>{sample.note[lang]}</p>}

      <label>
        <span>{t.input}</span>
        <textarea value={input} onChange={(e) => setInput(e.target.value)} spellCheck={false} rows={10} />
      </label>

      <div className={styles.outputs}>
        {outputs.map(({ id, name, via, out }) => (
          <section key={id} className={styles.output}>
            <header>
              <h3>{name}</h3>
              <code>{via}</code>
            </header>
            {"error" in out ? (
              <p className={styles.error}>{out.error}</p>
            ) : (
              <>
                <p className={styles.meta}>{t.parts(out.parts.length, out.limit)}</p>
                {out.parts.map((part, i) => (
                  <pre key={i}>{part || t.empty}</pre>
                ))}
                <RepairList repairs={out.repairs} lang={lang} />
              </>
            )}
          </section>
        ))}
      </div>
    </div>
  );
}

function RepairList({ repairs, lang }: { repairs: Repairs; lang: Locale }) {
  const t = text[lang];
  const fixed = keys.filter((k) => repairs[k] > 0);
  if (fixed.length === 0) return <p className={styles.repairs}>{t.noRepairs}</p>;
  return (
    <ul className={styles.repairs}>
      {fixed.map((k) => (
        <li key={k}>
          {t.repairs[k]} <b>×{repairs[k]}</b>
        </li>
      ))}
    </ul>
  );
}
