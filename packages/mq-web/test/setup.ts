import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import init from "../src/mq_wasm.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const wasmBytes = readFileSync(resolve(__dirname, "../src/mq_wasm_bg.wasm"));
await init({ module_or_path: wasmBytes.buffer as ArrayBuffer });
