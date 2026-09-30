// 같은 LLM 답변을 같은 토큰으로 흘려, 렌더러 다섯을 나란히 본다.
//
// - react-markdown: 누적본을 매번 통째로 다시 그린다
// - Streamdown: 누적본을 매번 다시 그리되, 안 닫힌 구문을 보정한다
// - mdwire → Streamdown: mdwire 스트리머가 정규화한 누적본 + 미리보기를 Streamdown 에 넘긴다(브릿지)
// - mdwire: useMarkdownStream — 기본(eager)은 붙든 것도 먼저 그린다
// - mdwire (eager: false): 확정된 것만 — 뒤 토큰이 앞 글을 안 고치는 대신 강조·표에서 멈춰 보인다
import { useEffect, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Streamdown } from "streamdown";
import { Streamer } from "@minjun0219/mdwire";
import { useMarkdownStream } from "@minjun0219/mdwire/react";
import { sample } from "./sample.js";

/** 샘플을 1~6글자 토큰으로 자른다 — LLM 토큰 크기쯤. 시드가 고정이라 매번 같게 잘린다. */
function tokenize(text) {
  const cps = [...text];
  const out = [];
  let seed = 7;
  for (let i = 0; i < cps.length; ) {
    // 32비트 정수 연산으로 — `seed * 1103515245` 는 안전 정수 범위를 넘어 아래 비트가 반올림돼,
    // 토큰 길이가 1·3·5 에 몰렸다(리뷰에서 나왔다). LCG 의 아래 비트는 주기가 짧아 위 비트를 쓴다.
    seed = (Math.imul(seed, 1103515245) + 12345) >>> 0;
    const n = 1 + ((seed >>> 16) % 6);
    out.push(cps.slice(i, i + n).join(""));
    i += n;
  }
  return out;
}
const TOKENS = tokenize(sample);

export function App() {
  const [run, setRun] = useState(0); // 바뀌면 처음부터 다시 흘린다
  const [delay, setDelay] = useState(60);
  return (
    <main>
      <header>
        <h1>mdwire — 스트리밍 비교</h1>
        <p>
          같은 답변을 같은 토큰({TOKENS.length}개)으로 흘린다. 반쪽 마커(<code>**굵</code>, 여는 백틱)가 글자로
          비쳤다 사라지는지, 조사 앞 굵게(<code>**설정(config)**을</code>)가 끝까지 그려지는지, 강조·표가 닫힐
          때까지 멈춰 보이는지 본다.
        </p>
        <label>
          토큰 간격 {delay}ms
          <input type="range" min="10" max="300" value={delay} onChange={(e) => setDelay(Number(e.target.value))} />
        </label>
        <button type="button" onClick={() => setRun((n) => n + 1)}>
          다시 흘리기
        </button>
      </header>
      <Panes key={run} delay={delay} />
    </main>
  );
}

function Panes({ delay }) {
  const [acc, setAcc] = useState("");
  const [bridged, setBridged] = useState("");
  const bridge = useRef(null);
  const mdwire = useMarkdownStream();
  const settled = useMarkdownStream({ eager: false });

  useEffect(() => {
    // 브릿지 — mdwire 가 GitHub 마크다운으로 정규화한 누적본에 미리보기 꼬리를 붙여 Streamdown 에 준다.
    bridge.current = new Streamer("github-markdown");
    let i = 0;
    let done = "";
    const timer = setInterval(() => {
      if (i >= TOKENS.length) {
        done += bridge.current.finish();
        setBridged(done);
        mdwire.finish();
        settled.finish();
        clearInterval(timer);
        return;
      }
      const tok = TOKENS[i++];
      setAcc((a) => a + tok);
      done += bridge.current.push(tok);
      setBridged(done + bridge.current.preview());
      mdwire.push(tok);
      settled.push(tok);
    }, delay);
    return () => {
      clearInterval(timer);
      bridge.current?.free();
    };
    // 간격을 바꿔도 이번 흐름은 그대로 — "다시 흘리기" 로 새로 시작한다.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="panes">
      <Pane title="react-markdown" note="누적본을 매번 다시 그림">
        <ReactMarkdown remarkPlugins={[remarkGfm]}>{acc}</ReactMarkdown>
      </Pane>
      <Pane title="Streamdown" note="누적본 + 안 닫힌 구문 보정">
        <Streamdown>{acc}</Streamdown>
      </Pane>
      <Pane title="mdwire → Streamdown" note="정규화한 누적본 + 미리보기">
        <Streamdown>{bridged}</Streamdown>
      </Pane>
      <Pane title="mdwire" note="useMarkdownStream()">
        {mdwire.elements}
      </Pane>
      <Pane title="mdwire" note="useMarkdownStream({ eager: false })">
        {settled.elements}
      </Pane>
    </div>
  );
}

function Pane({ title, note, children }) {
  return (
    <section className="pane">
      <h2>
        {title} <small>{note}</small>
      </h2>
      <div className="body">{children}</div>
    </section>
  );
}
