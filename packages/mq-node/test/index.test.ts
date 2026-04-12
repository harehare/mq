import { describe, it, expect } from "vitest";
import {
  run,
  format,
  toAst,
  toHtml,
  htmlToMarkdown,
  diagnostics,
  inlayHints,
  definedValues,
} from "../src/index";

describe("run", () => {
  it("filters list items with .[]", async () => {
    const result = await run(".[]", "- item 1\n- item 2");
    expect(result).toBe("- item 1\n- item 2\n");
  });

  it("changes list style to star", async () => {
    const markdown = "- First item\n- Second item\n- Third item";
    const result = await run(".[]", markdown, { listStyle: "star" });
    expect(result).toBe("* First item\n* Second item\n* Third item\n");
  });

  it("selects headings", async () => {
    const markdown = "# Title\n\nSome content.\n\n## Section\n\nMore content.";
    const result = await run(".h", markdown);
    expect(result).toBe("# Title\n## Section\n");
  });

  it("transforms text to uppercase", async () => {
    const result = await run(".[] | upcase()", "- apple\n- banana");
    expect(result).toBe("- APPLE\n- BANANA\n");
  });

  it("filters items by text content", async () => {
    const result = await run(
      '.[] | select(test(to_text(), "^A"))',
      "- Apple\n- Banana\n- Avocado",
    );
    expect(result).toBe("- Apple\n- Avocado\n");
  });
});

describe("format", () => {
  it("formats mq code with spaces around pipe", async () => {
    const result = await format("map(to_text)|select(gt(5))");
    expect(result).toBe("map(to_text) | select(gt(5))");
  });

  it("returns already-formatted code unchanged", async () => {
    const result = await format(".[]");
    expect(result).toBe(".[]");
  });
});

describe("toAst", () => {
  it("returns a non-empty AST string", async () => {
    const result = await toAst(".[]");
    expect(typeof result).toBe("string");
    expect(result.length).toBeGreaterThan(0);
  });
});

describe("toHtml", () => {
  it("converts heading to HTML", async () => {
    const result = await toHtml("# Hello\n\nWorld");
    expect(result).toContain("<h1>Hello</h1>");
    expect(result).toContain("<p>World</p>");
  });

  it("converts list to HTML", async () => {
    const result = await toHtml("- item 1\n- item 2");
    expect(result).toContain("<ul>");
    expect(result).toContain("<li>item 1</li>");
    expect(result).toContain("<li>item 2</li>");
  });
});

describe("htmlToMarkdown", () => {
  it("converts h1 and paragraph to markdown", async () => {
    const result = await htmlToMarkdown(
      "<h1>Hello World</h1><p>This is a paragraph.</p>",
    );
    expect(result).toContain("# Hello World");
    expect(result).toContain("This is a paragraph.");
  });

  it("converts unordered list to markdown", async () => {
    const result = await htmlToMarkdown(
      "<ul><li>Easy to use</li><li>Fast</li></ul>",
    );
    expect(result).toContain("* Easy to use");
    expect(result).toContain("* Fast");
  });
});

describe("diagnostics", () => {
  it("returns empty array for valid code", async () => {
    const result = await diagnostics(".[]");
    expect(result).toEqual([]);
  });

  it("returns diagnostics for invalid code", async () => {
    const result = await diagnostics("!!invalid!!");
    expect(result.length).toBeGreaterThan(0);
    expect(result[0]).toHaveProperty("message");
    expect(result[0]).toHaveProperty("startLine");
  });
});

describe("inlayHints", () => {
  it("returns an array for valid code", async () => {
    const result = await inlayHints(".[]");
    expect(Array.isArray(result)).toBe(true);
  });
});

describe("definedValues", () => {
  it("returns an array for empty code", async () => {
    const result = await definedValues("");
    expect(Array.isArray(result)).toBe(true);
  });

  it("returns defined function", async () => {
    const result = await definedValues("def double(x): x * 2;");
    expect(result.some((v) => v.name === "double")).toBe(true);
  });
});
