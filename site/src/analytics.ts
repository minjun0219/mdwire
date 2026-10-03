// 방문 집계(PostHog). 키와 프록시 주소는 빌드 변수 `POSTHOG_KEY` · `POSTHOG_HOST` 로 들어온다(스키마는 `astro.config.mjs`,
// 넣는 곳은 `site/README.md` 배포 절). 하나라도 없으면 초기화하지 않는다 — 그래서 변수를 안 준 로컬 · 프리뷰 · 포크 빌드는
// 아무것도 보내지 않고, 아래 import 도 같이 떨어져 나가 SDK 코드를 담지 않는다.
import { POSTHOG_HOST, POSTHOG_KEY } from "astro:env/client";

export function initAnalytics() {
  // 지역 상수에 받아야 아래 콜백 안에서도 `string` 으로 좁혀진 채 남는다.
  const key = POSTHOG_KEY;
  const host = POSTHOG_HOST;
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
