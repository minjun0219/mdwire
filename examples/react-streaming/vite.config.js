import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import wasm from "vite-plugin-wasm";

// mdwire 의 번들러 빌드는 `.wasm` 을 ESM 으로 import 한다(wasm-pack bundler 타깃).
// Vite 는 그걸 모르니 vite-plugin-wasm 을 켜고, 그 import 가 쓰는 top-level await 가
// 되도록 목표를 esnext 로 둔다. 번들러에서 mdwire 를 쓰는 앱이 같은 두 줄을 넣는다.
export default defineConfig({
  plugins: [react(), wasm()],
  build: { target: "esnext" },
  // 예제만의 설정 — `file:../../pkg` 는 심볼릭 링크라 패키지 안의 `import "react"` 가 저장소 쪽에서
  // react 를 찾다 못 찾는다. 앱의 react 하나로 맞춘다. npm 에서 받은 패키지는 필요 없다.
  resolve: { dedupe: ["react", "react-dom"] },
  optimizeDeps: { exclude: ["@minjun0219/mdwire"] },
});
