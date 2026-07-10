import { defineConfig } from "@vscode/test-cli";

export default defineConfig({
  files: "out/test/**/*.test.js",
  mocha: {
    ui: "tdd",
    timeout: 30_000,
  },
  env: {
    // Points the extension at a non-existent LSP binary so `activate()`
    // resolves quickly instead of prompting to install mq during tests.
    _MQ_DEBUG_BIN: "/nonexistent/mq-lsp-test-stub",
  },
});
