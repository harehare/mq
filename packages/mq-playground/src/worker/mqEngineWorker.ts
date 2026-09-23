import * as mq from "mq-web";

export type MqMethod =
  | "run"
  | "toAst"
  | "format"
  | "diagnostics"
  | "inlayHints"
  | "hover"
  | "definedValues"
  | "htmlToMarkdown"
  | "toHtml"
  | "clearHttpCache"
  | "clearAllHttpCache";

export interface MqWorkerRequest {
  id: number;
  method: MqMethod;
  args: unknown[];
}

export interface MqWorkerResponse {
  id: number;
  ok: boolean;
  result?: unknown;
  error?: string;
}

function wrap<A extends unknown[], R>(fn: (...args: A) => Promise<R>) {
  return (...args: unknown[]) => fn(...(args as A));
}

const handlers: Record<MqMethod, (...args: unknown[]) => Promise<unknown>> = {
  run: wrap(mq.run),
  toAst: wrap(mq.toAst),
  format: wrap(mq.format),
  diagnostics: wrap(mq.diagnostics),
  inlayHints: wrap(mq.inlayHints),
  hover: wrap(mq.hover),
  definedValues: wrap(mq.definedValues),
  htmlToMarkdown: wrap(mq.htmlToMarkdown),
  toHtml: wrap(mq.toHtml),
  clearHttpCache: wrap(mq.clearHttpCache),
  clearAllHttpCache: wrap(mq.clearAllHttpCache),
};

const ctx = self as unknown as Worker;

ctx.onmessage = async (event: MessageEvent<MqWorkerRequest>) => {
  const { id, method, args } = event.data;

  try {
    const result = await handlers[method](...args);
    ctx.postMessage({ id, ok: true, result } satisfies MqWorkerResponse);
  } catch (e) {
    ctx.postMessage({
      id,
      ok: false,
      error: e instanceof Error ? e.message : String(e),
    } satisfies MqWorkerResponse);
  }
};
