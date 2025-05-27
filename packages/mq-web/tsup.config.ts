import { defineConfig } from "tsup";

export default defineConfig({
  entry: ["src/index.ts", "src/core.ts"],
  format: ["esm", "cjs"],
  dts: true,
  clean: true,
  sourcemap: false,
  minify: true,
  publicDir: "mq-wasm",
  target: "es2020",
  splitting: false,
  bundle: true,
});
