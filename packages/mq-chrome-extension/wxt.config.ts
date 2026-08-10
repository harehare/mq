import { defineConfig } from "wxt";

export default defineConfig({
  modules: ["@wxt-dev/module-react", "@wxt-dev/auto-icons"],
  srcDir: ".",
  manifest: {
    name: "mq for Markdown",
    description:
      "Convert the current page to Markdown, filter it with mq queries, and preview the result on the page.",
    minimum_chrome_version: "116",
    permissions: ["activeTab", "scripting", "sidePanel", "storage"],
    action: {},
    content_security_policy: {
      extension_pages:
        "script-src 'self' 'wasm-unsafe-eval'; object-src 'self'",
    },
  },
});
