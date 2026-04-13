import { resolve } from "path";
import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    alias: {
      // In test environment, mq_wasm.cjs is the compiled output; map it to the
      // source mq_wasm.js so vitest can load the CJS WASM module directly.
      "./mq_wasm.cjs": resolve(__dirname, "src/mq_wasm.js"),
    },
  },
  test: {
    environment: "node",
  },
});
