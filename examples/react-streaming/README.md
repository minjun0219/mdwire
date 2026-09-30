# React 스트리밍 비교

같은 LLM 답변(합성)을 같은 토큰으로 흘려 렌더러 다섯을 나란히 본다 — react-markdown, Streamdown,
mdwire → Streamdown(브릿지, 미리보기), mdwire `useMarkdownStream`(기본 미리보기), 같은 훅의 `eager: false`(확정분만). 수치 비교는 `DESIGN.md` 의 "브릿지" 절.

```sh
./scripts/build-npm.sh          # 저장소 루트에서 — 예제는 로컬 pkg/ 를 쓴다
cd examples/react-streaming
npm install
npm run dev
```

**번들러에서 mdwire 를 쓸 때** 필요한 설정이 `vite.config.js` 에 있다 — mdwire 는 `.wasm` 을 ESM 으로
import 해서 `vite-plugin-wasm` 과 `build.target: "esnext"` 가 필요하다.
