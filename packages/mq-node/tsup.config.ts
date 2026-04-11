import { defineConfig } from "tsup";

export default defineConfig({
  entry: ["src/index.ts", "src/core.ts"],
  format: ["esm"],
  dts: true,
  clean: true,
  sourcemap: false,
  minify: true,
  publicDir: "mq-wasm",
  target: "node18",
  splitting: false,
  bundle: true,
  external: ["./mq-wasm/mq_wasm.js"],
});
