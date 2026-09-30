import type { ElementType, ReactElement, ReactNode } from "react";
import type { MdTag } from "./events.js";
import type { RenderOptions } from "@minjun0219/mdwire";

/** 태그별로 갈아 끼울 컴포넌트 — 예: `{ a: MyLink, code: CodeBlock }`. 받는 props 는 그 태그의 것이다. */
export type MdComponents = Partial<Record<MdTag, ElementType>>;

export interface MarkdownProps {
  /** 에이전트가 쓴 마크다운(완성된 글). 스트리밍은 `Streamer` + `toElements` 로 — 누적본을 매번 넘기면 처음부터 다시 변환한다. */
  text: string;
  /** 입력 표기. 슬랙 레거시 mrkdwn 이면 "slack-mrkdwn". */
  from?: "markdown" | "slack-mrkdwn";
  components?: MdComponents;
  /** 코어에 넘길 옵션 — `html` 정책(줄바꿈·이미지·스킴)이 여기 든다. `from` 은 위 것이 이긴다. */
  options?: RenderOptions;
}

/** 마크다운을 React 요소로 그린다. innerHTML 을 쓰지 않는다. */
export function Markdown(props: MarkdownProps): ReactElement;

export interface MarkdownStreamOptions {
  from?: "markdown" | "slack-mrkdwn";
  components?: MdComponents;
  options?: RenderOptions;
}

/**
 * 스트리밍용 훅 — 토큰마다 `push`, 끝나면 `finish`. 이미 보인 것은 뒤 토큰이 고치지 않는다.
 * 옵션은 처음 한 번만 읽는다.
 */
export function useMarkdownStream(opts?: MarkdownStreamOptions): {
  elements: ReactElement;
  push(chunk: string): void;
  finish(): void;
};

/** html 채널 출력(스트리밍이면 누적본 + `closeOpen()`)을 React 노드로 바꾼다. */
/** `schemes` 는 코어에 준 `options.html.schemes` 와 같게 준다. 기본 `["http", "https", "mailto"]`. */
export function toElements(html: string, components?: MdComponents, schemes?: string[]): ReactNode[];
