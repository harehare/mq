import { resolve } from "path";
import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    alias: {
      "./mq_wasm.cjs": resolve(__dirname, "src/mq_wasm.js"),
    },
  },
  test: {
    environment: "node",
    setupFiles: ["./test/setup.ts"],
  },
});
