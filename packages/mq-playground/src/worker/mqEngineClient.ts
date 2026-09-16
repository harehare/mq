import type {
  Options,
  Diagnostic,
  InlayHint,
  HoverResult,
  DefinedValue,
} from "mq-web";
import type {
  MqMethod,
  MqWorkerRequest,
  MqWorkerResponse,
} from "./mqEngineWorker";

export type {
  Options,
  Diagnostic,
  InlayHint,
  HoverResult,
  DefinedValue,
  DefinedValueType,
} from "mq-web";

export class MqCancelledError extends Error {
  constructor() {
    super("Execution was stopped");
    this.name = "MqCancelledError";
  }
}

interface PendingCall {
  resolve: (value: unknown) => void;
  reject: (reason: unknown) => void;
}

let worker: Worker | null = null;
let nextId = 0;
const pending = new Map<number, PendingCall>();

function handleMessage(event: MessageEvent<MqWorkerResponse>) {
  const { id, ok, result, error } = event.data;
  const entry = pending.get(id);
  if (!entry) {
    return;
  }
  pending.delete(id);
  if (ok) {
    entry.resolve(result);
  } else {
    entry.reject(new Error(error));
  }
}

function handleWorkerError(event: ErrorEvent) {
  for (const [id, entry] of pending) {
    entry.reject(new Error(event.message || "mq worker crashed"));
    pending.delete(id);
  }
  event.preventDefault();
}

function ensureWorker(): Worker {
  if (worker) {
    return worker;
  }
  worker = new Worker(new URL("./mqEngineWorker.ts", import.meta.url), {
    type: "module",
  });
  worker.onmessage = handleMessage;
  worker.onerror = handleWorkerError;
  return worker;
}

function call<T>(method: MqMethod, args: unknown[]): Promise<T> {
  const w = ensureWorker();
  const id = nextId++;
  return new Promise<T>((resolve, reject) => {
    pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
    const request: MqWorkerRequest = { id, method, args };
    w.postMessage(request);
  });
}

export function cancel(): void {
  if (!worker) {
    return;
  }
  worker.terminate();
  worker = null;
  for (const [id, entry] of pending) {
    entry.reject(new MqCancelledError());
    pending.delete(id);
  }
}

export function isCancelledError(error: unknown): boolean {
  return error instanceof MqCancelledError;
}

export function run(
  code: string,
  content: string,
  options: Partial<Options> = {},
): Promise<string> {
  return call<string>("run", [code, content, options]);
}

export function toAst(code: string): Promise<string> {
  return call<string>("toAst", [code]);
}

export function format(code: string): Promise<string> {
  return call<string>("format", [code]);
}

export function diagnostics(
  code: string,
  enableTypeCheck?: boolean,
): Promise<ReadonlyArray<Diagnostic>> {
  return call<ReadonlyArray<Diagnostic>>("diagnostics", [
    code,
    enableTypeCheck,
  ]);
}

export function inlayHints(code: string): Promise<ReadonlyArray<InlayHint>> {
  return call<ReadonlyArray<InlayHint>>("inlayHints", [code]);
}

export function hover(
  code: string,
  line: number,
  column: number,
): Promise<HoverResult | null> {
  return call<HoverResult | null>("hover", [code, line, column]);
}

export function definedValues(
  code: string,
  module?: string,
): Promise<ReadonlyArray<DefinedValue>> {
  return call<ReadonlyArray<DefinedValue>>("definedValues", [code, module]);
}

export function htmlToMarkdown(html: string, options?: unknown): Promise<string> {
  return call<string>("htmlToMarkdown", [html, options]);
}

export function toHtml(markdownInput: string): Promise<string> {
  return call<string>("toHtml", [markdownInput]);
}

export function clearHttpCache(): Promise<void> {
  return call<void>("clearHttpCache", []);
}

export function clearAllHttpCache(): Promise<void> {
  return call<void>("clearAllHttpCache", []);
}
