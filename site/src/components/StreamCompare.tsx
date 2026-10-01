// 같은 LLM 답변을 같은 토큰으로 흘려 렌더러 다섯을 나란히 본다 — `examples/react-streaming` 을 옮겼다.
//
// - react-markdown: 누적본을 매번 통째로 다시 그린다
// - Streamdown: 누적본을 매번 다시 그리되, 안 닫힌 구문을 보정한다
// - mdwire → Streamdown: mdwire 스트리머가 정규화한 누적본 + 미리보기를 Streamdown 에 넘긴다(브릿지)
// - mdwire: useMarkdownStream — 기본(eager)은 붙든 것도 먼저 그린다
// - mdwire (eager: false): 확정된 것만 — 뒤 토큰이 앞 글을 안 고치는 대신 강조·표에서 멈춰 보인다
import { useEffect, useRef, useState, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Streamdown } from "streamdown";
import { Streamer } from "@minjun0219/mdwire";
import { useMarkdownStream } from "@minjun0219/mdwire/react";
import type { Locale } from "../i18n";
import { streamSample, tokenize } from "./streamSample";
import "./stream-compare.css";

const TOKENS = tokenize(streamSample);

const text = {
  en: {
    delay: (ms: number) => `Token interval ${ms} ms`,
    restart: "Stream again",
    tokens: (n: number) => `${n} tokens`,
    panes: [
      { title: "react-markdown", note: "redraws the accumulated text every time" },
      { title: "Streamdown", note: "accumulated text + unclosed-syntax repair" },
      { title: "mdwire → Streamdown", note: "normalized output + preview" },
      { title: "mdwire", note: "useMarkdownStream()" },
      { title: "mdwire", note: "useMarkdownStream({ eager: false })" },
    ],
  },
  ko: {
    delay: (ms: number) => `토큰 간격 ${ms}ms`,
    restart: "다시 흘리기",
    tokens: (n: number) => `토큰 ${n}개`,
    panes: [
      { title: "react-markdown", note: "누적본을 매번 다시 그림" },
      { title: "Streamdown", note: "누적본 + 안 닫힌 구문 보정" },
      { title: "mdwire → Streamdown", note: "정규화한 누적본 + 미리보기" },
      { title: "mdwire", note: "useMarkdownStream()" },
      { title: "mdwire", note: "useMarkdownStream({ eager: false })" },
    ],
  },
} as const;

export default function StreamCompare({ lang }: { lang: Locale }) {
  const t = text[lang];
  const [run, setRun] = useState(0); // 바뀌면 처음부터 다시 흘린다
  const [delay, setDelay] = useState(60);
  // 데모 페이지 아래쪽에 있어서, 화면에 들어온 뒤에 흘리기 시작한다 — 미리 흘리면 내려왔을 땐 끝나 있다.
  const root = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const io = new IntersectionObserver(([e]) => {
      if (e.isIntersecting) {
        setVisible(true);
        io.disconnect();
      }
    }, { threshold: 0.2 });
    io.observe(el);
    return () => io.disconnect();
  }, []);
  return (
    <div className="sc" ref={root}>
      <div className="sc-controls">
        <label>
          <span>{t.delay(delay)}</span>
          <input type="range" min={10} max={300} value={delay} onChange={(e) => setDelay(Number(e.target.value))} />
        </label>
        <button type="button" onClick={() => setRun((n) => n + 1)}>
          {t.restart}
        </button>
        <span className="sc-meta">{t.tokens(TOKENS.length)}</span>
      </div>
      {visible && <Panes key={run} delay={delay} lang={lang} />}
    </div>
  );
}

function Panes({ delay, lang }: { delay: number; lang: Locale }) {
  const panes = text[lang].panes;
  const [acc, setAcc] = useState("");
  const [bridged, setBridged] = useState("");
  const bridge = useRef<Streamer | null>(null);
  const mdwire = useMarkdownStream();
  const settled = useMarkdownStream({ eager: false });

  useEffect(() => {
    // 브릿지 — mdwire 가 GitHub 마크다운으로 정규화한 누적본에 미리보기 꼬리를 붙여 Streamdown 에 준다.
    const s = new Streamer("github-markdown");
    bridge.current = s;
    let i = 0;
    let done = "";
    const timer = setInterval(() => {
      if (i >= TOKENS.length) {
        done += s.finish();
        setBridged(done);
        mdwire.finish();
        settled.finish();
        clearInterval(timer);
        return;
      }
      const tok = TOKENS[i++];
      setAcc((a) => a + tok);
      done += s.push(tok);
      setBridged(done + s.preview());
      mdwire.push(tok);
      settled.push(tok);
    }, delay);
    return () => {
      clearInterval(timer);
      s.free();
      bridge.current = null;
    };
    // 간격을 바꿔도 이번 흐름은 그대로 — "다시 흘리기" 로 새로 시작한다.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const bodies: ReactNode[] = [
    <ReactMarkdown remarkPlugins={[remarkGfm]}>{acc}</ReactMarkdown>,
    // 표·코드 블록의 복사·다운로드·전체화면 버튼은 끈다 — 그리는 방식만 비교한다.
    <Streamdown controls={false}>{acc}</Streamdown>,
    <Streamdown controls={false}>{bridged}</Streamdown>,
    mdwire.elements,
    settled.elements,
  ];

  return (
    <div className="sc-panes">
      {panes.map((p, i) => (
        <section key={i} className="sc-pane">
          <h3>
            {p.title} <small>{p.note}</small>
          </h3>
          <div className="sc-body">{bodies[i]}</div>
        </section>
      ))}
    </div>
  );
}
