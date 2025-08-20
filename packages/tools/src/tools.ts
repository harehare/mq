import { run } from "mq-web";
import type { Tool } from "./types";

interface MqWebOptions {
  inputFormat: "html" | "markdown" | "text" | "null" | "raw";
}

interface TransformError extends Error {
  name: "TransformError";
  originalError: unknown;
}

const createTransformError = (error: unknown): TransformError => {
  const transformError = new Error() as TransformError;
  transformError.name = "TransformError";
  transformError.originalError = error;

  if (error instanceof Error) {
    transformError.message = error.message.includes("WebAssembly")
      ? "WebAssembly module failed to initialize. Please refresh the page and try again."
      : error.message;
  } else {
    transformError.message = "An unknown error occurred during transformation";
  }

  return transformError;
};

const safeRun = async (
  query: string,
  input: string,
  options: MqWebOptions
): Promise<string> => {
  try {
    return await run(query, input, options);
  } catch (error) {
    console.error("mq-web execution error:", error);
    throw createTransformError(error);
  }
};

export const tools: Tool[] = [
  {
    id: "csv-to-markdown",
    name: "CSV to Markdown Table",
    description: "Convert CSV data to a Markdown table.",
    path: "/csv-to-markdown",
    category: "Conversion",
    transform: async (input: string): Promise<string> =>
      safeRun(
        `include "csv" | csv_parse(false) | csv_to_markdown_table()`,
        input,
        { inputFormat: "raw" }
      ),
  },
  {
    id: "html-to-markdown",
    name: "HTML to Markdown",
    description: "Convert HTML to Markdown format.",
    path: "/html-to-markdown",
    category: "Conversion",
    transform: async (input: string): Promise<string> =>
      safeRun("identity()", input, { inputFormat: "html" }),
  },
  {
    id: "json-to-markdown",
    name: "JSON to Markdown",
    description: "Convert JSON data to a Markdown table.",
    path: "/json-to-markdown",
    category: "Conversion",
    transform: async (input: string): Promise<string> =>
      safeRun(
        `include "json" | json_parse() | json_to_markdown_table()`,
        input,
        { inputFormat: "raw" }
      ),
  },
  {
    id: "markdown-code-extractor",
    name: "Markdown Code Extractor",
    description: "Extract all code blocks from Markdown.",
    path: "/code-extractor",
    category: "Extraction",
    transform: async (input: string): Promise<string> =>
      safeRun(".code", input, { inputFormat: "markdown" }),
  },
  {
    id: "markdown-link-extractor",
    name: "Markdown Link Extractor",
    description: "Extract all links (URLs) from Markdown.",
    path: "/link-extractor",
    category: "Extraction",
    transform: async (input: string): Promise<string> =>
      safeRun(".link.url", input, { inputFormat: "markdown" }),
  },
  {
    id: "markdown-to-html",
    name: "Markdown to HTML",
    description: "Convert Markdown to HTML format.",
    path: "/markdown-to-html",
    category: "Conversion",
    transform: async (input: string): Promise<string> =>
      safeRun("to_html()", input, { inputFormat: "markdown" }),
  },
  {
    id: "markdown-to-toc",
    name: "Markdown to TOC",
    description: "Generate a Table of Contents from Markdown.",
    path: "/markdown-to-toc",
    category: "Generation",
    transform: async (input: string): Promise<string> =>
      safeRun(
        `.h | let link = to_link("#" + to_text(self), to_text(self), "") | let level = .h.level | if (not(is_none(level))): to_md_list(link, to_number(level))`,
        input,
        { inputFormat: "markdown" }
      ),
  },
].map((tool) => ({
  ...tool,
  path: `/tools${tool.path}`,
})) as Tool[];
