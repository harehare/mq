import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  optimizeDeps: {
    exclude: ["mq-web"],
  },
  server: {
    fs: {
      allow: [".."],
    },
  },
  build: {
    outDir: "../../docs",
    assetsDir: "toolsAssets",
    rollupOptions: {
      input: "tools.html",
    },
  },
  publicDir: "public",
});
