export type ExampleQuery = {
  name: string;
  code: string;
};

/**
 * A small curated subset of common queries, in the spirit of
 * packages/mq-playground/src/examples.ts (kept intentionally short for a
 * side panel's limited space).
 */
export const EXAMPLE_QUERIES: readonly ExampleQuery[] = [
  { name: "Headings", code: ".h" },
  { name: "Links", code: ".link" },
  { name: "Code blocks", code: ".code" },
  { name: "All elements", code: ".[]" },
];
