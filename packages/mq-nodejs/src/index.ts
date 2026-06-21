/**
 * mq-nodejs - A jq-like command-line tool for Markdown processing for Node.js
 *
 * This package provides async/await support for using mq in Node.js environments.
 */

// Re-export everything from core and types
export { run, format, diagnostics, inlayHints, definedValues, hover, toAst, htmlToMarkdown, toHtml, clearHttpCache, clearAllHttpCache } from "./core.js";

export type {
  Options,
  Diagnostic,
  InlayHint,
  HoverResult,
  DefinedValue,
  DefinedValueType,
} from "./mq_wasm.js";
