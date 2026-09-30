import type { ElementType, ReactElement, ReactNode } from "react";
import type { MdTag } from "./events.js";
import type { RenderOptions } from "@minjun0219/mdwire";

/** 태그별로 갈아 끼울 컴포넌트 — 예: `{ a: MyLink, code: CodeBlock }`. 받는 props 는 그 태그의 것이다. */
export type MdComponents = Partial<Record<MdTag, ElementType>>;

export interface MarkdownProps {
  /** 에이전트가 쓴 마크다운. 스트리밍이면 지금까지의 누적본. */
  text: string;
  /** 입력 표기. 슬랙 레거시 mrkdwn 이면 "slack-mrkdwn". */
  from?: "markdown" | "slack-mrkdwn";
  components?: MdComponents;
  /** 코어에 넘길 옵션 — `html` 정책(줄바꿈·이미지·스킴)이 여기 든다. `from` 은 위 것이 이긴다. */
  options?: RenderOptions;
}

/** 마크다운을 React 요소로 그린다. innerHTML 을 쓰지 않는다. */
export function Markdown(props: MarkdownProps): ReactElement;

/** html 채널 출력(스트리밍이면 누적본 + `closeOpen()`)을 React 노드로 바꾼다. */
/** `schemes` 는 코어에 준 `options.html.schemes` 와 같게 준다. 기본 `["http", "https", "mailto"]`. */
export function toElements(html: string, components?: MdComponents, schemes?: string[]): ReactNode[];
