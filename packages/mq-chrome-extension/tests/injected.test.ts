import { describe, expect, it } from "vitest";
import { extractPageHtml } from "../lib/injected";

describe("extractPageHtml", () => {
  it("returns the current document's outer HTML", () => {
    document.body.innerHTML = "<p>hello</p>";
    expect(extractPageHtml()).toBe(document.documentElement.outerHTML);
    expect(extractPageHtml()).toContain("<p>hello</p>");
  });
});
