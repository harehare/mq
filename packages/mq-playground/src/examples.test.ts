import { describe, it, expect } from "vitest";
import { vi } from "vitest";
import { EXAMPLE_CATEGORIES, EXAMPLES } from "./examples";

vi.mock("mq-web", () => ({}));

describe("EXAMPLE_CATEGORIES", () => {
  it("is a non-empty array", () => {
    expect(Array.isArray(EXAMPLE_CATEGORIES)).toBe(true);
    expect(EXAMPLE_CATEGORIES.length).toBeGreaterThan(0);
  });

  it("each category has a name and non-empty examples array", () => {
    for (const category of EXAMPLE_CATEGORIES) {
      expect(typeof category.name).toBe("string");
      expect(category.name.length).toBeGreaterThan(0);
      expect(Array.isArray(category.examples)).toBe(true);
      expect(category.examples.length).toBeGreaterThan(0);
    }
  });

  it("each example has required fields", () => {
    for (const category of EXAMPLE_CATEGORIES) {
      for (const example of category.examples) {
        expect(typeof example.name).toBe("string");
        expect(example.name.length).toBeGreaterThan(0);
        expect(typeof example.code).toBe("string");
        expect(example.code.length).toBeGreaterThan(0);
        expect(typeof example.isUpdate).toBe("boolean");
        expect(["markdown", "mdx", "raw", "null"]).toContain(example.format);
      }
    }
  });

  it("markdown examples have non-empty markdown content", () => {
    for (const category of EXAMPLE_CATEGORIES) {
      for (const example of category.examples) {
        if (example.format === "markdown" || example.format === "mdx" || example.format === "raw") {
          expect(typeof example.markdown).toBe("string");
          expect(example.markdown.trim().length).toBeGreaterThan(0);
        }
      }
    }
  });

  it("example names are unique within each category", () => {
    for (const category of EXAMPLE_CATEGORIES) {
      const names = category.examples.map((e) => e.name);
      const unique = new Set(names);
      expect(unique.size).toBe(names.length);
    }
  });
});

describe("EXAMPLES", () => {
  it("is the flattened union of all category examples", () => {
    const expected = EXAMPLE_CATEGORIES.flatMap((c) => c.examples);
    expect(EXAMPLES).toEqual(expected);
  });

  it("total count matches sum of category counts", () => {
    const total = EXAMPLE_CATEGORIES.reduce(
      (sum, c) => sum + c.examples.length,
      0,
    );
    expect(EXAMPLES.length).toBe(total);
  });
});
