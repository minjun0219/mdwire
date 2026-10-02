// 정규화 전후 — 깨진 에이전트 출력을 넣으면 mdwire 가 무엇을 고쳤는지 원문과 맞대어 보여 준다.
// 예시는 코퍼스에서 실제로 정규화가 일어나는 케이스만 골랐다. 변환은 브라우저의 wasm 이 한다.
import { useMemo, useState } from "react";
import { renderWithReport } from "@minjun0219/mdwire";
import type { Locale } from "../i18n";
import { samples } from "./samples";
import styles from "./RepairCompare.module.css";

// 코퍼스 케이스 중 정규화 보고의 고친 것(앞 넷)이 0 이 아닌 것.
const ids = [
  "chat-bot-answer",
  "emphasis-across-linebreak",
  "unclosed-code-fence",
  "unpaired-backtick-run",
  "table-cell-overflow",
] as const;
const picks = ids.map((id) => samples.find((s) => s.id === id)!);

// 마크다운을 내는 채널은 원문과 글자 단위로 맞대 볼 수 있다. 텔레그램은 태그라 출력만 보인다.
const channels = [
  { id: "slack-markdown", name: "Slack markdown_text", diff: true },
  { id: "github-markdown", name: "GitHub (GFM)", diff: true },
  { id: "notion-markdown", name: "Notion", diff: true },
  { id: "telegram-html", name: "Telegram HTML", diff: false },
] as const;
type ChannelId = (typeof channels)[number]["id"];

// 정규화 보고 — 앞 넷은 고친 것, 뒤 여섯은 채널에 맞춰 바꾼 것.
const fixed = ["closedEmphasis", "closedFence", "revertedCodeSpan", "droppedMarker"] as const;
const rewritten = [
  "escapedChar",
  "tagEmphasis",
  "strippedHtml",
  "rewrittenBullet",
  "rewrittenTable",
  "convertedMarker",
] as const;
const keys = [...fixed, ...rewritten];
type Repairs = Record<(typeof keys)[number], number>;

// 고장이 무엇이었고 고치지 않으면 어떻게 되는가 — 각 케이스의 `why.md` 를 줄인 것.
const why: Record<(typeof ids)[number], Record<Locale, string>> = {
  "chat-bot-answer": {
    en: "Bold opened on “영향 범위” never closes, and neither does the code fence. Sent as is, the ** shows as text and Telegram rejects the open <pre>. mdwire closes the bold at the end of its block and the fence at the end of the answer.",
    ko: "“영향 범위”에서 연 굵게도, 코드펜스도 닫히지 않았습니다. 그대로 보내면 ** 가 텍스트로 보이고, 텔레그램은 열린 <pre> 를 거절합니다. mdwire 는 굵게를 그 블록 끝에서, 펜스를 답변 끝에서 닫습니다.",
  },
  "emphasis-across-linebreak": {
    en: "The ** after “채널” has no partner. A regex converter pairs it with the next ** and inverts the bold range — 28 of 60 samples. mdwire drops the stray marker and keeps the real bold.",
    ko: "“채널” 뒤의 ** 는 짝이 없습니다. 정규식 기반 변환기는 이를 다음 ** 와 짝지어 굵게 범위를 뒤집습니다(표본 60개 중 28개). mdwire 는 짝 없는 마커를 버리고 진짜 굵게만 남깁니다.",
  },
  "unclosed-code-fence": {
    en: "The answer ended inside a fence (cut off at the token limit). Sent as is, <pre> stays open: the channel rejects it or swallows whatever follows. mdwire closes it right before output.",
    ko: "답변이 코드펜스 안에서 끝났습니다(토큰 한도에서 잘림). 그대로 보내면 <pre> 가 열린 채라 채널이 거절하거나 뒤의 내용을 통째로 삼킵니다. mdwire 는 출력 직전에 닫습니다.",
  },
  "unpaired-backtick-run": {
    en: "A ``` with no partner. Opened as a code span, it swallows the rest of the line, bold included. mdwire keeps the run as text, so the Markdown barely changes — switch to Telegram HTML to see the bold after it come out as <b>.",
    ko: "짝 없는 ``` 입니다. 이를 코드 스팬으로 열면 줄 끝까지 통째로 삼켜서 뒤의 굵게도 코드 안에 갇힙니다. mdwire 는 이 백틱을 텍스트로 둡니다. 마크다운은 거의 그대로이므로, 채널을 텔레그램 HTML 로 바꿔 보면 뒤의 굵게가 <b> 로 나오는 것을 확인할 수 있습니다.",
  },
  "table-cell-overflow": {
    en: "The | inside `vol|wlv` splits the cell, leaving two half code spans (kept as text) and one cell too many — which GFM silently drops. mdwire folds the extra cell into the last one with an escaped \\|, so the author's last cell survives.",
    ko: "`vol|wlv` 안의 | 가 칸을 나눠서, 반쪽짜리 코드 스팬 두 개(텍스트로 되돌림)와 넘치는 칸 하나가 생깁니다. GFM 은 넘치는 칸을 아무 경고 없이 버립니다. mdwire 는 넘치는 칸을 이스케이프한 \\| 로 마지막 칸에 합쳐서 저자가 쓴 마지막 칸을 살립니다.",
  },
};

const text = {
  en: {
    sample: "Broken output from the test cases",
    channel: "Channel",
    input: "Agent output — edit it",
    output: "What mdwire sends",
    legend: "added by mdwire · removed by mdwire",
    added: "added",
    removed: "removed",
    unchanged: "The text is unchanged.",
    report: "Repair report",
    rows: {
      closedEmphasis: ["closedEmphasis", "emphasis left open at the end of its block, closed"],
      closedFence: ["closedFence", "code fence left open at the end of the document, closed"],
      revertedCodeSpan: ["revertedCodeSpan", "backtick run with no partner, kept as text"],
      droppedMarker: ["droppedMarker", "stray ** with nothing to pair with, dropped"],
      escapedChar: ["escapedChar", "characters the channel would read as syntax, escaped"],
      tagEmphasis: ["tagEmphasis", "emphasis the channel cannot read as markers, written another way (GitHub <strong>, Slack U+2060)"],
      strippedHtml: ["strippedHtml", "source HTML the channel cannot draw, stripped"],
      rewrittenBullet: ["rewrittenBullet", "list markers rewritten for the channel"],
      rewrittenTable: ["rewrittenTable", "tables rewritten (rows tidied, or a monospace block)"],
      convertedMarker: ["convertedMarker", "emphasis and links rewritten into another notation"],
    },
    groups: ["Repaired", "Rewritten for the channel"],
    empty: "(empty)",
  },
  ko: {
    sample: "테스트 케이스의 깨진 출력",
    channel: "채널",
    input: "에이전트 출력 (직접 고쳐 보세요)",
    output: "mdwire 가 보내는 결과",
    legend: "mdwire 가 더한 것 · 뺀 것",
    added: "더함",
    removed: "뺌",
    unchanged: "텍스트는 바뀌지 않았습니다.",
    report: "정규화 보고",
    rows: {
      closedEmphasis: ["closedEmphasis", "블록 끝까지 닫히지 않은 강조를 닫았습니다"],
      closedFence: ["closedFence", "문서 끝까지 닫히지 않은 코드펜스를 닫았습니다"],
      revertedCodeSpan: ["revertedCodeSpan", "짝 없는 연속된 백틱을 텍스트로 되돌렸습니다"],
      droppedMarker: ["droppedMarker", "짝이 없는 ** 를 버렸습니다"],
      escapedChar: ["escapedChar", "채널이 구문으로 읽을 글자를 이스케이프했습니다"],
      tagEmphasis: ["tagEmphasis", "마커로 읽히지 않는 강조를 다르게 출력했습니다(GitHub 의 <strong>, 슬랙의 U+2060)"],
      strippedHtml: ["strippedHtml", "채널이 렌더링하지 못하는 원문 HTML 을 제거했습니다"],
      rewrittenBullet: ["rewrittenBullet", "목록 기호를 채널에 맞게 바꿨습니다"],
      rewrittenTable: ["rewrittenTable", "표를 다시 썼습니다(줄 정리 또는 고정폭 블록)"],
      convertedMarker: ["convertedMarker", "강조와 링크를 다른 표기로 바꿨습니다"],
    },
    groups: ["고친 것", "채널에 맞춰 바꾼 것"],
    empty: "(비어 있음)",
  },
} as const;

type Output = { text: string; repairs: Repairs } | { error: string };

function convert(input: string, channel: ChannelId): Output {
  try {
    const out = renderWithReport(input, channel);
    const r = out.repairs;
    const repairs = Object.fromEntries(keys.map((k) => [k, r[k]])) as Repairs;
    // 조각으로 나뉘어도 여기선 하나로 본다 — 맞대 볼 것은 글자다.
    const text = out.parts.join("");
    r.free();
    out.free();
    return { text, repairs };
  } catch (e) {
    return { error: e instanceof Error ? e.message : String(e) };
  }
}

type Op = { kind: "same" | "add" | "del"; text: string };

// 글자 단위 LCS. 낱말 단위로 자르면 `채널**이다` 에서 `**` 를 뺀 `채널이다` 가 통째로 다른 낱말이 된다 —
// 한국어는 마커가 낱말 가운데 붙는다. 끝의 줄바꿈은 내용이 아니라 맞대지 않는다. 데모 입력은 짧다 —
// 표가 너무 커지면(직접 붙여 넣은 긴 글) 맞대기를 건너뛴다.
function diff(a: string, b: string): Op[] | undefined {
  const x = Array.from(a.replace(/\n+$/, ""));
  const y = Array.from(b.replace(/\n+$/, ""));
  const n = x.length;
  const m = y.length;
  if (n * m > 2_000_000) return undefined;
  const w = m + 1;
  const dp = new Uint32Array((n + 1) * w);
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i * w + j] = x[i] === y[j] ? dp[(i + 1) * w + j + 1] + 1 : Math.max(dp[(i + 1) * w + j], dp[i * w + j + 1]);
    }
  }
  const ops: Op[] = [];
  const push = (kind: Op["kind"], t: string) => {
    const last = ops[ops.length - 1];
    if (last?.kind === kind) last.text += t;
    else ops.push({ kind, text: t });
  };
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (x[i] === y[j]) {
      push("same", x[i]);
      i++;
      j++;
    } else if (dp[(i + 1) * w + j] >= dp[i * w + j + 1]) {
      push("del", x[i++]);
    } else {
      push("add", y[j++]);
    }
  }
  while (i < n) push("del", x[i++]);
  while (j < m) push("add", y[j++]);
  return ops;
}

export default function RepairCompare({ lang }: { lang: Locale }) {
  const t = text[lang];
  const [sampleId, setSampleId] = useState<(typeof ids)[number]>(ids[0]);
  const [input, setInput] = useState(picks[0].input);
  const [channel, setChannel] = useState<ChannelId>("slack-markdown");
  const sample = picks.find((s) => s.id === sampleId)!;
  const showDiff = channels.find((c) => c.id === channel)!.diff;

  const out = useMemo(() => convert(input, channel), [input, channel]);
  const ops = useMemo(
    () => ("error" in out || !showDiff ? undefined : diff(input, out.text)),
    [input, out, showDiff],
  );
  const changed = ops?.some((op) => op.kind !== "same");

  function pick(id: (typeof ids)[number]) {
    setSampleId(id);
    setInput(picks.find((s) => s.id === id)!.input);
  }

  return (
    <div className={styles.root}>
      <div className={styles.controls}>
        <label>
          <span>{t.sample}</span>
          <select value={sampleId} onChange={(e) => pick(e.target.value as (typeof ids)[number])}>
            {picks.map((s) => (
              <option key={s.id} value={s.id}>
                {s.id}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>{t.channel}</span>
          <select value={channel} onChange={(e) => setChannel(e.target.value as ChannelId)}>
            {channels.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
        </label>
      </div>
      {sample.input === input && <p className={styles.note}>{why[sampleId][lang]}</p>}

      <div className={styles.panes}>
        <label className={styles.pane}>
          <span>{t.input}</span>
          <textarea value={input} onChange={(e) => setInput(e.target.value)} spellCheck={false} rows={9} />
        </label>
        <section className={styles.pane}>
          <span>{t.output}</span>
          {"error" in out ? (
            <p className={styles.error}>{out.error}</p>
          ) : ops ? (
            <pre className={styles.output}>
              {ops.map((op, k) =>
                op.kind === "same" ? (
                  op.text
                ) : op.kind === "add" ? (
                  <ins key={k} title={t.added}>
                    {op.text}
                  </ins>
                ) : (
                  <del key={k} title={t.removed}>
                    {op.text}
                  </del>
                ),
              )}
            </pre>
          ) : (
            <pre className={styles.output}>{out.text || t.empty}</pre>
          )}
          {ops && (
            <p className={styles.legend}>
              {changed ? (
                <>
                  <ins>{t.added}</ins> <del>{t.removed}</del>
                </>
              ) : (
                t.unchanged
              )}
            </p>
          )}
        </section>
      </div>

      {!("error" in out) && (
        <table className={styles.report}>
          <caption>{t.report}</caption>
          {[fixed, rewritten].map((group, g) => (
            <tbody key={g}>
              <tr>
                <th colSpan={3} scope="rowgroup" className={styles.group}>
                  {t.groups[g]}
                </th>
              </tr>
              {group.map((k) => (
                <tr key={k} className={out.repairs[k] > 0 ? styles.hit : undefined}>
                  <th scope="row">
                    <code>{t.rows[k][0]}</code>
                  </th>
                  <td className={styles.count}>{out.repairs[k]}</td>
                  <td>{t.rows[k][1]}</td>
                </tr>
              ))}
            </tbody>
          ))}
        </table>
      )}
    </div>
  );
}
