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

/**
 * Toggles a full-viewport overlay that renders `srcdoc` inside a fully
 * sandboxed iframe. Calling this again removes the overlay, restoring the
 * original page view. The overlay lives in a closed shadow root and is
 * built entirely via `createElement`/CSSOM property assignment/
 * `addEventListener` (never `<style>` tags, `style="..."` strings, or
 * inline `on*` attributes) so it isn't affected by, or subject to, the
 * host page's own CSS or Content-Security-Policy `style-src`/`script-src`
 * directives.
 *
 * Returns whether the overlay is now visible.
 */
export function toggleOverlayPreview(srcdoc: string): boolean {
  const hostId = "__mq_preview_overlay_host__";
  const existing = document.getElementById(hostId);

  if (existing) {
    existing.remove();
    document.documentElement.style.removeProperty("overflow");
    return false;
  }

  const host = document.createElement("div");
  host.id = hostId;
  host.style.setProperty("all", "initial");
  host.style.setProperty("position", "fixed");
  host.style.setProperty("inset", "0");
  host.style.setProperty("z-index", "2147483647");

  const shadow = host.attachShadow({ mode: "closed" });

  const container = document.createElement("div");
  container.style.setProperty("width", "100%");
  container.style.setProperty("height", "100%");
  container.style.setProperty("display", "flex");
  container.style.setProperty("flex-direction", "column");

  const header = document.createElement("div");
  header.style.setProperty("display", "flex");
  header.style.setProperty("align-items", "center");
  header.style.setProperty("justify-content", "space-between");
  header.style.setProperty("height", "40px");
  header.style.setProperty("flex", "0 0 auto");
  header.style.setProperty("padding", "0 12px");
  header.style.setProperty("box-sizing", "border-box");
  header.style.setProperty("background", "#1e293b");
  header.style.setProperty("color", "#e2e8f0");
  header.style.setProperty(
    "font-family",
    "-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif",
  );
  header.style.setProperty("font-size", "13px");

  const label = document.createElement("span");
  label.textContent = "mq preview";

  const closeButton = document.createElement("button");
  closeButton.type = "button";
  closeButton.textContent = "✕ Exit preview";
  closeButton.style.setProperty("background", "transparent");
  closeButton.style.setProperty("border", "1px solid #475569");
  closeButton.style.setProperty("border-radius", "4px");
  closeButton.style.setProperty("color", "#e2e8f0");
  closeButton.style.setProperty("cursor", "pointer");
  closeButton.style.setProperty("font-size", "12px");
  closeButton.style.setProperty("padding", "4px 10px");
  closeButton.addEventListener("click", () => {
    host.remove();
    document.documentElement.style.removeProperty("overflow");
  });

  header.append(label, closeButton);

  const iframe = document.createElement("iframe");
  iframe.setAttribute("sandbox", "");
  iframe.style.setProperty("border", "none");
  iframe.style.setProperty("width", "100%");
  iframe.style.setProperty("flex", "1 1 auto");
  iframe.style.setProperty("background", "#fff");
  iframe.srcdoc = srcdoc;

  container.append(header, iframe);
  shadow.append(container);
  document.documentElement.append(host);
  document.documentElement.style.setProperty("overflow", "hidden");

  return true;
}
