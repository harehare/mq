// @vitest-environment node
import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { EXAMPLE_QUERIES } from "../entrypoints/popup/lib/examples";

const SAMPLE = `# Title

Intro with [ext](https://example.com/a), [rel](/docs/x), [dup](https://example.com/a), [anchor](#top) and \`inline_code\`.

## What is it?

![logo](https://img/x.png "t")

![](https://img/y.png)

\`\`\`rust
fn main() {}
\`\`\`

\`\`\`bash
npm install mq
\`\`\`

## Installation

Run this:

\`\`\`sh
cargo install mq
\`\`\`

| name | age |
| ---- | --- |
| a    | 1   |

- [ ] todo
- [x] done

> quote
`;

type Run = typeof import("mq-web").run;
let run: Run;

beforeAll(async () => {
  // mq-web loads its wasm through fetch(new URL(...)), which Node cannot do for file: URLs.
  const nativeFetch = globalThis.fetch;
  globalThis.fetch = (async (input, init) => {
    const url = String(input);
    if (url.startsWith("file:")) {
      return new Response(readFileSync(new URL(url)), {
        headers: { "Content-Type": "application/wasm" },
      });
    }
    return nativeFetch(input, init);
  }) as typeof fetch;
  ({ run } = await import("mq-web"));
});

describe("EXAMPLE_QUERIES against a sample document", () => {
  it.each(EXAMPLE_QUERIES.map((example) => [example.name, example.code]))(
    "%s runs and produces output",
    async (_name, code) => {
      const output = await run(code, SAMPLE, {});
      expect(output.trim().length).toBeGreaterThan(0);
    },
  );

  it.each([
    ["External links", "https://example.com/a\nhttps://example.com/a"],
    ["Relative links", "/docs/x"],
    ["Unique link URLs", "https://example.com/a\n/docs/x\n#top"],
    ["Unique code languages", "rust\nbash\nsh"],
    ["Shell commands", "npm install mq\ncargo install mq"],
    ["Question headings", "## What is it?"],
    ["First table as CSV", "name,age\na,1"],
  ])("%s returns the expected result", async (name, expected) => {
    const example = EXAMPLE_QUERIES.find((e) => e.name === name);
    expect(example, `example "${name}" not found`).toBeDefined();
    const output = await run(example!.code, SAMPLE, {});
    expect(output.trim()).toBe(expected);
  });

  it("Outline keeps heading nesting", async () => {
    const example = EXAMPLE_QUERIES.find((e) => e.name === "Outline");
    const output = await run(example!.code, SAMPLE, {});
    expect(output.trim()).toBe(
      "- Title\n  - What is it?\n  - Installation",
    );
  });
});
