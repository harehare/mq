/**
 * Wraps an HTML fragment (mq's `toHtml` output) into a full HTML document
 * string suitable for a fully sandboxed `<iframe sandbox="" srcdoc="...">`.
 * Adapted from mq-playground's `buildPreviewSrcDoc`
 * (packages/mq-playground/src/Playground.tsx), trimmed to a single
 * `prefers-color-scheme` media query instead of an app-level theme picker.
 */
export function buildSrcDoc(htmlFragment: string): string {
  return `<!DOCTYPE html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data: blob:;"><style>
:root{--bg:#ffffff;--fg:#1a1a1a;--pre-bg:#f5f5f5;--link:#0070c1;--heading:#1a1a1a;--border:#e0e0e0}
@media (prefers-color-scheme: dark){:root{--bg:#1e1e1e;--fg:#d4d4d4;--pre-bg:#2d2d2d;--link:#4ec9b0;--heading:#d4d4d4;--border:#3e3e42}}
body{margin:16px;font-family:sans-serif;background:var(--bg);color:var(--fg);line-height:1.6}
h1,h2,h3,h4,h5,h6{color:var(--heading);border-bottom:1px solid var(--border);padding-bottom:0.3em}
a{color:var(--link)}
pre{background:var(--pre-bg);padding:12px;border-radius:4px;overflow:auto;border:1px solid var(--border)}
code{font-family:'JetBrains Mono',monospace;font-size:0.9em}
blockquote{border-left:4px solid var(--border);margin:0;padding:0 1em;color:var(--fg);opacity:0.8}
table{border-collapse:collapse;width:100%}
th,td{border:1px solid var(--border);padding:6px 12px}
th{background:var(--pre-bg)}
img{max-width:100%}
</style></head><body>${htmlFragment}</body></html>`;
}
