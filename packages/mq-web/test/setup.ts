import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

// mq-web ships a wasm-pack "web" target, which loads its .wasm binary via
// `fetch(new URL(...))`. Node's built-in fetch cannot resolve file: URLs, so
// under Vitest we serve the local .wasm file ourselves and fall back to the
// real fetch for everything else.
const originalFetch = globalThis.fetch;

globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
  const url = input instanceof Request ? input.url : input.toString();

  if (url.startsWith("file:") && url.endsWith(".wasm")) {
    const bytes = readFileSync(fileURLToPath(url));
    return new Response(bytes, {
      status: 200,
      headers: { "Content-Type": "application/wasm" },
    });
  }

  return originalFetch(input, init);
}) as typeof fetch;
