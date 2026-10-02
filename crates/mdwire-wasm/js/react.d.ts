import type { ElementType, ReactElement, ReactNode } from "react";
import type { MdTag } from "./events.js";
import type { RenderOptions } from "@minjun0219/mdwire";

/** Components that replace individual tags, e.g. `{ a: MyLink, code: CodeBlock }`. Each receives that tag's props. */
export type MdComponents = Partial<Record<MdTag, ElementType>>;

export interface MarkdownProps {
  /** Markdown written by the agent (complete text). For streaming, use `Streamer` + `toElements`; passing the accumulated output each time reconverts it from scratch. */
  text: string;
  components?: MdComponents;
  /** Options passed to the core. The `html` policy (line breaks, images, schemes) goes here. */
  options?: RenderOptions;
}

/** Renders markdown as React elements. Does not use innerHTML. */
export function Markdown(props: MarkdownProps): ReactElement;

export interface MarkdownStreamOptions {
  components?: MdComponents;
  options?: RenderOptions;
  /** Also draw what is held back (open emphasis, table rows, code spans) ahead of time. Default `true`. With `false`, only final output is shown. */
  eager?: boolean;
  /** Called when done. `revised` tells whether the final output differs from the last screen; if false, the hook does not redraw. */
  onSettled?: (html: string, revised: boolean) => void;
}

/**
 * Hook for streaming: call `push` for each token and `finish` at the end. By default it also draws what is held back (`eager`).
 * Options other than `onSettled` are read only once, on first render.
 */
export function useMarkdownStream(opts?: MarkdownStreamOptions): {
  elements: ReactElement;
  push(chunk: string): void;
  finish(): void;
};

/** Converts html channel output (when streaming, the accumulated output + `preview()` or `closeOpen()`) into React nodes. */
/** Pass the same `schemes` you gave the core as `options.html.schemes`. Default `["http", "https", "mailto"]`. */
export function toElements(html: string, components?: MdComponents, schemes?: string[]): ReactNode[];
