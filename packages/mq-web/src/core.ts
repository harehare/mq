import init, { Options, Diagnostic, DefinedValue } from "./mq_wasm";

// Type definitions for WASM module
interface WasmModule {
  run: (code: string, content: string, options: Options) => string;
  toAst: (code: string) => string;
  format: (code: string) => string;
  diagnostics: (code: string) => readonly Diagnostic[];
  definedValues: (code: string, module?: string) => readonly DefinedValue[];
}

let wasmModule: WasmModule | null = null;

async function initWasm(): Promise<WasmModule> {
  if (wasmModule) {
    return wasmModule;
  }

  await (async () => {
    try {
      await init();
      const wasmImport = await import("./mq_wasm.js");

      wasmModule = {
        run: wasmImport.run,
        toAst: wasmImport.toAst,
        format: wasmImport.format,
        diagnostics: wasmImport.diagnostics,
        definedValues: wasmImport.definedValues,
      };
    } catch (error) {
      throw new Error(`Failed to initialize mq WebAssembly module: ${error}`);
    }
  })();

  return wasmModule!;
}

/**
 * Run an mq
 */
export async function run(
  code: string,
  content: string,
  options: Partial<Options> = {}
): Promise<string> {
  const wasm = await initWasm();
  return await wasm.run(code, content, {
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
  const wasm = await initWasm();
  return await wasm.toAst(code);
}

/**
 * Format mq code
 */
export async function format(code: string): Promise<string> {
  const wasm = await initWasm();
  return await wasm.format(code);
}

/**
 * Get diagnostics for mq code
 */
export async function diagnostics(
  code: string
): Promise<ReadonlyArray<Diagnostic>> {
  const wasm = await initWasm();
  return await wasm.diagnostics(code);
}

/**
 * Get defined values from mq code
 */
export async function definedValues(
  code: string,
  module?: string
): Promise<ReadonlyArray<DefinedValue>> {
  const wasm = await initWasm();
  return await wasm.definedValues(code, module);
}
