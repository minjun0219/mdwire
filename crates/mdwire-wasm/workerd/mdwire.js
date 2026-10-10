// Cloudflare Workers(workerd) 진입점. `exports` 의 `workerd` 조건이 여기로 온다.
//
// **번들러 빌드를 그대로 쓰고 wasm 을 싣는 방법만 바꾼다.** `bundler/mdwire.js` 는
// `import * as wasm from "./mdwire_bg.wasm"` 이 인스턴스의 export 를 준다고 가정한다(webpack 의
// wasm ESM 통합). wrangler(esbuild)는 `.wasm` import 를 컴파일된 `WebAssembly.Module` 하나로,
// 그것도 default export 로 준다 — 그대로 두면 `wasm.*` 가 전부 undefined 라 첫 호출에서 죽는다.
// Workers 는 바이트에서 컴파일하는 것은 막지만 컴파일된 Module 의 동기 인스턴스화는 허용한다.
import wasmModule from "../bundler/mdwire_bg.wasm";
import * as glue from "../bundler/mdwire_bg.js";

glue.__wbg_set_wasm(new WebAssembly.Instance(wasmModule, { "./mdwire_bg.js": glue }).exports);

// 내보내는 이름은 빌드가 `bundler/mdwire.js` 의 export 줄을 옮겨 붙인다 — 여기 따로 적으면 API 가 늘 때 어긋난다.
