// WebMCP — 이 사이트를 연 브라우저 에이전트에게 mdwire 를 도구로 내준다(https://github.com/webmachinelearning/webmcp).
// 명세는 `document.modelContext`, 초기 구현은 `navigator.modelContext` 였다 — 있는 쪽을 쓰고, 없으면 아무것도 하지 않는다.
// wasm 은 도구가 처음 불릴 때 받는다. 도구를 등록만 하는 페이지는 무겁지 않다.

const channels = ["telegram-html", "slack-markdown", "github-markdown", "notion-markdown", "plain", "html"] as const;

// 정규화 보고의 열 가지 수 — ChannelCompare 와 같다.
const repairKeys = [
  "closedEmphasis",
  "closedFence",
  "revertedCodeSpan",
  "droppedMarker",
  "escapedChar",
  "tagEmphasis",
  "strippedHtml",
  "rewrittenBullet",
  "rewrittenTable",
  "convertedMarker",
] as const;

interface ModelContext {
  registerTool(tool: {
    name: string;
    description: string;
    inputSchema: object;
    execute(input: Record<string, unknown>): Promise<{ content: { type: "text"; text: string }[]; isError?: boolean }>;
  }): unknown;
}

const text = (value: string, isError = false) => ({ content: [{ type: "text" as const, text: value }], isError });

export function registerWebMcp() {
  const mc = (document as unknown as { modelContext?: ModelContext }).modelContext ??
    (navigator as unknown as { modelContext?: ModelContext }).modelContext;
  if (typeof mc?.registerTool !== "function") return;

  mc.registerTool({
    name: "mdwire_render",
    description:
      "Convert LLM-generated Markdown into what a chat channel accepts, the way mdwire does: repair broken markup " +
      "(unclosed ** or code fences), rewrite it for the channel and split it within the channel's length limit. " +
      "Returns JSON: the parts to send (one message each) and the repair report. Runs locally in this page.",
    inputSchema: {
      type: "object",
      properties: {
        markdown: { type: "string", description: "The Markdown as the model wrote it. Do not pre-escape it." },
        channel: {
          type: "string",
          enum: channels,
          description:
            'Where it goes: telegram-html (parse_mode "HTML"), slack-markdown (markdown_text), github-markdown, ' +
            "notion-markdown, plain, or html (safe for innerHTML).",
        },
        limit: { type: "integer", minimum: 256, description: "Characters per part. Defaults to the channel's limit." },
      },
      required: ["markdown", "channel"],
    },
    async execute(input) {
      const { markdown, channel, limit } = input;
      // 스키마는 모델에게 주는 힌트일 뿐이라 여기서 다시 본다.
      if (typeof markdown !== "string") return text("markdown must be a string", true);
      if (!channels.includes(channel as (typeof channels)[number])) {
        return text(`channel must be one of: ${channels.join(", ")}`, true);
      }
      if (limit !== undefined && (!Number.isInteger(limit) || (limit as number) < 1)) {
        return text("limit must be a positive integer", true);
      }
      try {
        const mdwire = await import("@minjun0219/mdwire");
        const out = mdwire.renderWithReport(markdown, channel as string, limit ? { limit: limit as number } : undefined);
        // wasm 쪽 객체라 읽고 나서 바로 놓는다.
        const r = out.repairs;
        const repairs = Object.fromEntries(repairKeys.map((k) => [k, r[k]]));
        const parts = out.parts;
        r.free();
        out.free();
        return text(JSON.stringify({ channel, parts, repairs }));
      } catch (e) {
        return text(e instanceof Error ? e.message : String(e), true);
      }
    },
  });
}
