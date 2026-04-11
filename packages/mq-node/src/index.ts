/**
 * mq-node - A jq-like command-line tool for Markdown processing for Node.js
 *
 * This package provides async/await support for using mq in Node.js environments.
 */

// Re-export everything from core and types
export { run, format, diagnostics, inlayHints, definedValues, toAst, htmlToMarkdown, toHtml } from "./core.js";

export type {
  Options,
  Diagnostic,
  InlayHint,
  DefinedValue,
  DefinedValueType,
} from "../mq-wasm/mq_wasm.js";
