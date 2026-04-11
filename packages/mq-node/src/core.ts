import * as wasmImport from "./mq-wasm/mq_wasm.js";
import {
  Options,
  Diagnostic,
  DefinedValue,
  InlayHint,
  ConversionOptions,
} from "./mq-wasm/mq_wasm.js";

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
  definedValues: (
    code: string,
    module?: string,
  ) => Promise<readonly DefinedValue[]>;
  htmlToMarkdown(
    html_input: string,
    options?: ConversionOptions,
  ): Promise<string>;
  toHtml(markdown_input: string): Promise<string>;
}

const wasmModule: WasmModule = {
  run: wasmImport.run,
  toAst: wasmImport.toAst,
  format: wasmImport.format,
  diagnostics: wasmImport.diagnostics,
  inlayHints: wasmImport.inlayHints,
  definedValues: wasmImport.definedValues,
  htmlToMarkdown: wasmImport.htmlToMarkdown,
  toHtml: wasmImport.toHtml,
};

/**
 * Run an mq
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
 * Format mq code
 */
export async function format(code: string): Promise<string> {
  return await wasmModule.format(code);
}

/**
 * Get diagnostics for mq code
 */
export async function diagnostics(
  code: string,
  enableTypeCheck?: boolean,
): Promise<ReadonlyArray<Diagnostic>> {
  return await wasmModule.diagnostics(code, enableTypeCheck);
}

/**
 * Get inlay type hints for mq code
 */
export async function inlayHints(
  code: string,
): Promise<ReadonlyArray<InlayHint>> {
  return await wasmModule.inlayHints(code);
}

/**
 * Get defined values from mq code
 */
export async function definedValues(
  code: string,
  module?: string,
): Promise<ReadonlyArray<DefinedValue>> {
  return await wasmModule.definedValues(code, module);
}

/**
 * Converts HTML input to Markdown
 */
export async function htmlToMarkdown(
  html: string,
  options?: ConversionOptions,
): Promise<string> {
  return await wasmModule.htmlToMarkdown(html, options);
}

/**
 * Markdown Input to HTML
 */
export async function toHtml(markdownInput: string): Promise<string> {
  return await wasmModule.toHtml(markdownInput);
}
