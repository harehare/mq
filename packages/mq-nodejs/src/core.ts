import * as wasmImport from "./mq_wasm.js";
import type {
  Options,
  Diagnostic,
  DefinedValue,
  InlayHint,
  HoverResult,
  ConversionOptions,
} from "./mq_wasm.js";

// Type definitions for WASM module
interface WasmModule {
  run: (code: string, content: string, options: Options) => Promise<string>;
  toAst: (code: string) => Promise<string>;
  format: (code: string) => Promise<string>;
  diagnostics: (
    code: string,
    enableTypeCheck?: boolean,
  ) => Promise<readonly Diagnostic[]>;
  inlayHints: (code: string) => Promise<readonly InlayHint[]>;
  hover: (
    code: string,
    line: number,
    column: number,
  ) => Promise<HoverResult | null>;
  definedValues: (
    code: string,
    module?: string,
  ) => Promise<readonly DefinedValue[]>;
  htmlToMarkdown(
    html_input: string,
    options?: ConversionOptions,
  ): Promise<string>;
  toHtml(markdown_input: string): Promise<string>;
  clearHttpCache(): Promise<void>;
  clearAllHttpCache(): Promise<void>;
}

const wasmModule: WasmModule = {
  run: wasmImport.run,
  toAst: wasmImport.toAst,
  format: wasmImport.format,
  diagnostics: wasmImport.diagnostics,
  inlayHints: wasmImport.inlayHints,
  hover: wasmImport.hover,
  definedValues: wasmImport.definedValues,
  htmlToMarkdown: wasmImport.htmlToMarkdown,
  toHtml: wasmImport.toHtml,
  clearHttpCache: wasmImport.clearHttpCache,
  clearAllHttpCache: wasmImport.clearAllHttpCache,
};

/**
 * Run an mq script on Markdown content.
 */
export async function run(
  code: string,
  content: string,
  options: Partial<Options> = {},
): Promise<string> {
  return await wasmModule.run(code, content, {
    isUpdate: false,
    inputFormat: "markdown",
    listStyle: "dash",
    linkUrlStyle: "none",
    linkTitleStyle: "paren",
    ...options,
  });
}

/**
 * Convert mq code to its AST (Abstract Syntax Tree) representation.
 */
export async function toAst(code: string): Promise<string> {
  return await wasmModule.toAst(code);
}

/**
 * Format mq code.
 */
export async function format(code: string): Promise<string> {
  return await wasmModule.format(code);
}

/**
 * Get diagnostics for mq code.
 */
export async function diagnostics(
  code: string,
  enableTypeCheck?: boolean,
): Promise<ReadonlyArray<Diagnostic>> {
  return await wasmModule.diagnostics(code, enableTypeCheck);
}

/**
 * Get inlay type hints for mq code.
 */
export async function inlayHints(
  code: string,
): Promise<ReadonlyArray<InlayHint>> {
  return await wasmModule.inlayHints(code);
}

/**
 * Get defined values (functions, selectors, variables) from mq code.
 */
export async function definedValues(
  code: string,
  module?: string,
): Promise<ReadonlyArray<DefinedValue>> {
  return await wasmModule.definedValues(code, module);
}

/**
 * Get hover information for the symbol at the given position (1-based line and column).
 *
 * Returns `null` when no symbol exists at the position.
 */
export async function hover(
  code: string,
  line: number,
  column: number,
): Promise<HoverResult | null> {
  return await wasmModule.hover(code, line, column);
}

/**
 * Convert HTML to Markdown.
 */
export async function htmlToMarkdown(
  html: string,
  options?: ConversionOptions,
): Promise<string> {
  return await wasmModule.htmlToMarkdown(html, options);
}

/**
 * Convert Markdown to HTML.
 */
export async function toHtml(markdownInput: string): Promise<string> {
  return await wasmModule.toHtml(markdownInput);
}

/**
 * Clears mutable HTTP module cache (HEAD/branch imports).
 * Versioned (tagged) cache is preserved.
 */
export async function clearHttpCache(): Promise<void> {
  return await wasmModule.clearHttpCache();
}

/**
 * Clears all HTTP module cache including versioned (tagged) imports.
 */
export async function clearAllHttpCache(): Promise<void> {
  return await wasmModule.clearAllHttpCache();
}
