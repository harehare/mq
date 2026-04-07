import { defineConfig } from "vite";
import react, { reactCompilerPreset } from "@vitejs/plugin-react";
import babel from "@rolldown/plugin-babel";

export default defineConfig({
  base: "./",
  plugins: [react(), babel({ presets: [reactCompilerPreset()] })],
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
