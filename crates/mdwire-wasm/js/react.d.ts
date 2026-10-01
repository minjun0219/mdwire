import type { ElementType, ReactElement, ReactNode } from "react";
import type { MdTag } from "./events.js";
import type { RenderOptions } from "@minjun0219/mdwire";

/** 태그별로 갈아 끼울 컴포넌트 — 예: `{ a: MyLink, code: CodeBlock }`. 받는 props 는 그 태그의 것이다. */
export type MdComponents = Partial<Record<MdTag, ElementType>>;

export interface MarkdownProps {
  /** 에이전트가 쓴 마크다운(완성된 글). 스트리밍은 `Streamer` + `toElements` 로 — 누적본을 매번 넘기면 처음부터 다시 변환한다. */
  text: string;
  components?: MdComponents;
  /** 코어에 넘길 옵션 — `html` 정책(줄바꿈·이미지·스킴)이 여기 든다. */
  options?: RenderOptions;
}

/** 마크다운을 React 요소로 그린다. innerHTML 을 쓰지 않는다. */
export function Markdown(props: MarkdownProps): ReactElement;

export interface MarkdownStreamOptions {
  components?: MdComponents;
  options?: RenderOptions;
  /** 붙든 것(열린 강조·표 행·코드 스팬)도 먼저 그린다. 기본 `true`. `false` 면 확정된 것만. */
  eager?: boolean;
  /** 끝났을 때. `revised` 는 완성본이 마지막 화면과 다른가 — 거짓이면 훅은 다시 그리지 않는다. */
  onSettled?: (html: string, revised: boolean) => void;
}

/**
 * 스트리밍용 훅 — 토큰마다 `push`, 끝나면 `finish`. 기본은 붙든 것도 먼저 그린다(`eager`).
 * `onSettled` 말고는 처음 한 번만 읽는다.
 */
export function useMarkdownStream(opts?: MarkdownStreamOptions): {
  elements: ReactElement;
  push(chunk: string): void;
  finish(): void;
};

/** html 채널 출력(스트리밍이면 누적본 + `preview()` 또는 `closeOpen()`)을 React 노드로 바꾼다. */
/** `schemes` 는 코어에 준 `options.html.schemes` 와 같게 준다. 기본 `["http", "https", "mailto"]`. */
export function toElements(html: string, components?: MdComponents, schemes?: string[]): ReactNode[];
