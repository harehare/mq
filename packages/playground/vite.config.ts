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
    },
  },
  optimizeDeps: { exclude: ["mq-web"] },
  server: {
    open: "/playground.html",
    fs: {
      allow: [".", "../packages"],
    },
  },
});
