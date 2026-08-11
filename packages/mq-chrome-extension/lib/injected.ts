/**
 * Functions in this file are passed directly as `func` to
 * `browser.scripting.executeScript`. Their bodies are serialized and run in
 * the target page's isolated world, so they must be fully self-contained:
 * no imports, no closures over anything outside the function itself.
 */

/** Returns the full HTML of the current page. */
export function extractPageHtml(): string {
  return document.documentElement.outerHTML;
}
