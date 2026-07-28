import { createRequire } from "node:module";
import path from "node:path";
import { defineConfig } from "vite";
import react, { reactCompilerPreset } from "@vitejs/plugin-react";
import babel from "@rolldown/plugin-babel";

const require = createRequire(import.meta.url);

// monaco-editor's "exports" map (as of 0.56.0) can't resolve the extensionless
// "esm/vs/..." deep import monaco-vim's UMD bundle requires, so derive the
// package root manually and point straight at the file on disk.
const monacoEditorRoot = path.resolve(
  path.dirname(require.resolve("monaco-editor")),
  "../..",
);

export default defineConfig({
  base: "./",
  plugins: [react(), babel({ presets: [reactCompilerPreset()] })],
  resolve: {
    alias: [
      {
        find: "monaco-editor/esm/vs/editor/editor.api",
        replacement: path.join(
          monacoEditorRoot,
          "esm/vs/editor/editor.api.js",
        ),
      },
    ],
  },
  build: {
    outDir: "../../docs",
    rollupOptions: {
      input: {
        main: "playground.html",
      },
      output: {
        manualChunks: (id) => {
          if (id.includes("monaco-editor")) {
            return "monaco";
          }
          if (id.includes("node_modules")) {
            return "vendor";
          }
        },
      },
    },
    chunkSizeWarningLimit: 1000,
    assetsInlineLimit: 4096,
  },
  optimizeDeps: { exclude: ["mq-web"] },
  server: {
    open: "/playground.html",
    fs: {
      allow: ["../..", ".", "../packages"],
    },
  },
});
