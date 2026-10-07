// 방문 집계(PostHog · Google Analytics 4). 키 · 프록시 주소 · 측정 ID 는 빌드 변수 `PUBLIC_POSTHOG_KEY` ·
// `PUBLIC_POSTHOG_HOST` · `PUBLIC_GA_MEASUREMENT_ID` 로 들어온다(스키마는 `astro.config.mjs`, 넣는 곳은 `site/README.md` 배포 절).
// 둘은 따로 켜진다 — 제 변수가 없으면 초기화하지 않는다. 그래서 변수를 안 준 로컬 · 프리뷰 · 포크 빌드는 아무것도 보내지 않고,
// PostHog 는 아래 import 도 같이 떨어져 나가 SDK 코드를 담지 않는다.
import { PUBLIC_GA_MEASUREMENT_ID, PUBLIC_POSTHOG_HOST, PUBLIC_POSTHOG_KEY } from "astro:env/client";

declare global {
  interface Window {
    dataLayer: unknown[];
    gtag: (...args: unknown[]) => void;
  }
}

export function initAnalytics() {
  initPostHog();
  initGoogleAnalytics();
}

function initPostHog() {
  // 지역 상수에 받아야 아래 콜백 안에서도 `string` 으로 좁혀진 채 남는다.
  const key = PUBLIC_POSTHOG_KEY;
  const host = PUBLIC_POSTHOG_HOST;
  if (!key || !host) return;
  // SDK 는 따로 떼어 받는다 — 레이아웃 스크립트(복사 버튼 · WebMCP)가 이 코드를 기다리지 않는다.
  // slim 번들이라 아래에서 끈 확장(세션 녹화 · 설문 · 자동 수집)의 코드도 없다.
  import("posthog-js/dist/module.slim").then(({ default: posthog }) => {
    posthog.init(key, {
      api_host: host,
      defaults: "2026-05-30",
      // 페이지뷰(와 떠남)만 — 클릭 자동 수집 · 세션 녹화 · 설문은 쓰지 않는다.
      autocapture: false,
      disable_session_recording: true,
      disable_surveys: true,
    });
  });
}

// Google 태그(gtag.js) 공식 스니펫을 옮긴 것. 명령은 `dataLayer` 에 먼저 쌓이고, 비동기로 받은 gtag.js 가 뒤늦게 읽는다 —
// 그래서 이 스크립트도 gtag.js 를 기다리지 않는다. 페이지뷰 외에 무엇을 더 모을지(향상된 측정)는 GA 관리 화면에서 정한다.
function initGoogleAnalytics() {
  const id = PUBLIC_GA_MEASUREMENT_ID;
  if (!id) return;
  window.dataLayer = window.dataLayer || [];
  // 스니펫 그대로 `arguments` 를 넣는다 — gtag.js 는 `arguments` 객체를 명령으로 읽는다. 나머지 매개변수(배열)로
  // 바꾸면 명령으로 읽히지 않는다.
  window.gtag = function gtag() {
    window.dataLayer.push(arguments);
  };
  window.gtag("js", new Date());
  window.gtag("config", id);
  const script = document.createElement("script");
  script.async = true;
  script.src = `https://www.googletagmanager.com/gtag/js?id=${encodeURIComponent(id)}`;
  document.head.append(script);
}
