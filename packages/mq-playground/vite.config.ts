import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  base: "./",
  plugins: [react()],
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
