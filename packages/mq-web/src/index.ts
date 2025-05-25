/**
 * mq-web - A jq-like command-line tool for Markdown processing (WebAssembly package)
 *
 * This package provides async/await support for using mq in web environments.
 *
 * @example
 * ```typescript
 * import { run, format, Mq } from 'mq-web';
 *
 * // One-off usage
 * const result = await run('.[]', '- item 1\n- item 2', { listStyle: 'star' });
 * console.log(result); // * item 1\n* item 2
 *
 * // Reusable instance
 * const mq = await Mq.create();
 * const formatted = await mq.format('map(.text)');
 * const diagnostics = await mq.diagnostics('invalid syntax');
 * ```
 */

// Re-export everything from core and types
export { run, format, diagnostics, definedValues } from "./core.js";

export type {
  Options,
  Diagnostic,
  DefinedValue,
  DefinedValueType,
} from "../mq-wasm/mq_wasm.js";
