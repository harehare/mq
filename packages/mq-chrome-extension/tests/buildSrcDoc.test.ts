import { describe, expect, it } from "vitest";
import { buildSrcDoc } from "../entrypoints/sidepanel/lib/buildSrcDoc";

describe("buildSrcDoc", () => {
  it("wraps the HTML fragment in a full document with a restrictive CSP", () => {
    const doc = buildSrcDoc("<h1>Hello</h1>");

    expect(doc).toContain("<!DOCTYPE html>");
    expect(doc).toContain('<body><h1>Hello</h1></body>');
    expect(doc).toContain(
      "Content-Security-Policy\" content=\"default-src 'none';",
    );
  });

  it("includes both light and dark color variables", () => {
    const doc = buildSrcDoc("<p>content</p>");

    expect(doc).toContain("--bg:#ffffff");
    expect(doc).toContain("prefers-color-scheme: dark");
    expect(doc).toContain("--bg:#1e1e1e");
  });
});
