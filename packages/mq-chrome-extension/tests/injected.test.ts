import { afterEach, describe, expect, it } from "vitest";
import { extractPageHtml, toggleOverlayPreview } from "../lib/injected";

const OVERLAY_HOST_ID = "__mq_preview_overlay_host__";

afterEach(() => {
  document.getElementById(OVERLAY_HOST_ID)?.remove();
  document.documentElement.style.removeProperty("overflow");
  document.body.innerHTML = "";
});

describe("extractPageHtml", () => {
  it("returns the current document's outer HTML", () => {
    document.body.innerHTML = "<p>hello</p>";
    expect(extractPageHtml()).toBe(document.documentElement.outerHTML);
    expect(extractPageHtml()).toContain("<p>hello</p>");
  });
});

describe("toggleOverlayPreview", () => {
  it("adds a closed-shadow overlay host and hides page overflow", () => {
    const isVisible = toggleOverlayPreview("<p>preview</p>");

    expect(isVisible).toBe(true);
    const host = document.getElementById(OVERLAY_HOST_ID);
    expect(host).not.toBeNull();
    expect(host?.shadowRoot).toBeNull();
    expect(document.documentElement.style.overflow).toBe("hidden");
  });

  it("removes the overlay and restores overflow when toggled again", () => {
    toggleOverlayPreview("<p>preview</p>");

    const isVisible = toggleOverlayPreview("<p>ignored on removal</p>");

    expect(isVisible).toBe(false);
    expect(document.getElementById(OVERLAY_HOST_ID)).toBeNull();
    expect(document.documentElement.style.overflow).toBe("");
  });
});
