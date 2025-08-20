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
    rollupOptions: {
      input: "tools.html",
      external: ["mq-web"],
    },
  },
  // Allow serving files from node_modules
  publicDir: "public",
  assetsInclude: ["**/*.wasm"],
});
