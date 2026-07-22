import { defineConfig } from "@vscode/test-cli";

export default defineConfig({
  files: "out/test/**/*.test.js",
  // Pinned instead of "stable" so @vscode/test-electron skips the network
  // call to resolve the latest release and downloads this version directly.
  version: "1.128.0",
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
