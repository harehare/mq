import { describe, expect, it } from "vitest";
import { EXAMPLE_QUERIES } from "../entrypoints/popup/lib/examples";

describe("EXAMPLE_QUERIES", () => {
  it("has a name and non-empty mq code for every entry", () => {
    expect(EXAMPLE_QUERIES.length).toBeGreaterThan(0);
    for (const example of EXAMPLE_QUERIES) {
      expect(example.name.length).toBeGreaterThan(0);
      expect(example.code.length).toBeGreaterThan(0);
    }
  });

  it("has unique names", () => {
    const names = EXAMPLE_QUERIES.map((example) => example.name);
    expect(new Set(names).size).toBe(names.length);
  });
});
