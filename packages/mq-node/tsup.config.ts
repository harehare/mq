import { defineConfig } from "tsup";

export default defineConfig({
  entry: ["src/index.ts", "src/core.ts"],
  format: ["esm", "cjs"],
  dts: true,
  clean: true,
  sourcemap: false,
  minify: true,
  publicDir: "mq-wasm",
  platform: "node",
  splitting: false,
  bundle: true,
  external: ["./mq_wasm"],
});
