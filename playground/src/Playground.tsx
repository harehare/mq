import { useState, useEffect, useCallback } from "react";
import Editor, { Monaco } from "@monaco-editor/react";
import "./index.css";

import init, { runScript, formatScript } from "./mq-wasm/mq_wasm";
import { FaGithub } from "react-icons/fa6";

const CODE_KEY = "mq-playground.code";
const MARKDOWN_KEY = "mq-playground.markdown";
const isDarkMode = window.matchMedia("(prefers-color-scheme: dark)").matches;
const EXAMPLES = [
  {
    name: "Hello World",
    code: `# Hello world
select(or(.[], .code, .h)) | upcase() | add(" Hello World")?`,
    markdown: `# h1

- item1
- item2

\`\`\`
code
\`\`\`
`,
  },
  {
    name: "Markdown Toc",
    code: `.h
| let link = md_link(add("#", to_text(self)), to_text(self))
| if (eq(md_name(), "h1")):
  md_list(link, 1)
elif (eq(md_name(), "h2")):
  md_list(link, 2)
elif (eq(md_name(), "h3")):
  md_list(link, 3)
elif (eq(md_name(), "h4")):
  md_list(link, 4)
elif (eq(md_name(), "h5")):
  md_list(link, 5)
else:
  None`,
    markdown: `# [header1](https://example.com)

- item 1
- item 2

## header2

- item 1
- item 2

### header3

- item 1
- item 2

#### header4

- item 1
- item 2`,
  },
  {
    name: "Extract js code",
    code: `.code("js")`,
    markdown: `# Sample codes
\`\`\`js
console.log("Hello, World!");
\`\`\`

\`\`\`python
print("Hello, World!")
\`\`\`

\`\`\`js
console.log("Hello, World!");
\`\`\`
`,
  },
  {
    name: "Custom function",
    code: `def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
      let first_char = upcase(first(word))
      | let rest_str = downcase(slice(word, 1, len(word)))
      | add(first_char, rest_str);
  | join("");
| snake_to_camel()`,
    markdown: `# sample_codes`,
  },
  {
    name: "Generate sitemap",
    code: `def sitemap(item, base_url):
  let url = "<url>
  <loc>\${loc}</loc>
</url>"
  | let path = replace(to_text(item), ".md", ".html")
  | replace(url, "\${loc}", add(base_url, path)); | sitemap("https://example.com/")`,
    markdown: `# Summary

- [Chapter1](chapter1.md)
- [Chapter2](Chapter2.md)
  - [Chapter3](Chapter3.md)
- [Chapter4](Chapter4.md)
`,
  },
  {
    name: "Extract table",
    code: `.[1][]`,
    markdown: `# Product List

| Product | Category | Price | Stock |
|---------|----------|-------|-------|
| Laptop  | Electronics | $1200 | 45 |
| Monitor | Electronics | $350 | 28 |
| Chair   | Furniture | $150 | 73 |
| Desk    | Furniture | $200 | 14 |
| Keyboard | Accessories | $80 | 35 |

| Product | Category | Price | Stock |
|---------|----------|-------|-------|
| Mouse   | Accessories | $25 | 50 |
| Headphones | Electronics | $120 | 32 |
| Bookshelf | Furniture | $180 | 17 |
| USB Cable | Accessories | $12 | 89 |
| Coffee Maker | Appliances | $85 | 24 |
`,
  },
  {
    name: "Extract list",
    code: `.[] | select(is_list2())`,
    markdown: `# Product List

- Electronics
  - Laptop: $1200
  - Monitor: $350
  - Headphones: $120
- Furniture
  - Chair: $150
  - Desk: $200
  - Bookshelf: $180
- Accessories
  - Keyboard: $80
  - Mouse: $25
  - USB Cable: $12
`,
  },
];

export const Playground = () => {
  const [code, setCode] = useState<string | undefined>(
    localStorage.getItem(CODE_KEY) || EXAMPLES[0].code
  );

  const [markdown, setMarkdown] = useState<string | undefined>(
    localStorage.getItem(MARKDOWN_KEY) || EXAMPLES[0].markdown
  );

  const [result, setResult] = useState("");
  const [wasmLoaded, setWasmLoaded] = useState(false);

  useEffect(() => {
    init().then(() => {
      setWasmLoaded(true);
    });
  }, []);

  const handleRun = useCallback(async () => {
    if (!code || !markdown) {
      return;
    }

    try {
      setResult(await runScript(code, markdown));
    } catch (e) {
      setResult((e as Error).toString());
    }
  }, [code, markdown]);

  const handleFormat = useCallback(async () => {
    if (!code) {
      return;
    }

    setCode(await formatScript(code));
  }, [code]);

  const handleChangeExample = useCallback((index: number) => {
    const selected = EXAMPLES[index];
    setCode(selected.code);
    setMarkdown(selected.markdown);
  }, []);

  const beforeMount = (monaco: Monaco) => {
    monaco.languages.register({ id: "mq" });
    monaco.languages.setMonarchTokensProvider("mq", {
      tokenizer: {
        root: [
          [/^#.*$/, "comment"],
          [/let|def|while|foreach|if|elif|else|self/, "keyword"],
          [/;/, "delimiter"],
          [/\|/, "operator"],
          [/".*?"/, "string"],
          [/\d+/, "number"],
          [/[a-zA-Z_]\w*(?=\s*\()/, "function"],
          [/(([a-zA-Z_]\w*)\s*\()/, "function"],
          [/\(|\)|\[|\]/, "delimiter.parenthesis"],
          [/[a-zA-Z_]\w*/, "identifier"],
        ],
      },
    });

    monaco.editor.defineTheme("mq-base", {
      base: isDarkMode ? "vs-dark" : "vs",
      inherit: true,
      rules: [
        {
          token: "comment",
          foreground: isDarkMode ? "#6A9955" : "#008000",
          fontStyle: "italic",
        },
        {
          token: "keyword",
          foreground: isDarkMode ? "#569CD6" : "#0000FF",
          fontStyle: "bold",
        },
        { token: "function", foreground: isDarkMode ? "#DCDCAA" : "#795E26" },
        { token: "variable", foreground: isDarkMode ? "#9CDCFE" : "#001080" },
        { token: "property", foreground: isDarkMode ? "#9CDCFE" : "#001080" },
        { token: "string", foreground: isDarkMode ? "#CE9178" : "#A31515" },
        { token: "number", foreground: isDarkMode ? "#B5CEA8" : "#098658" },
        {
          token: "operator",
          foreground: isDarkMode ? "#D4D4D4" : "#000000",
          fontStyle: "bold",
        },
        { token: "delimiter", foreground: isDarkMode ? "#D4D4D4" : "#000000" },
        { token: "identifier", foreground: isDarkMode ? "#D4D4D4" : "#000000" },
      ],
      colors: isDarkMode
        ? {
            "editor.background": "#1E1E1E",
            "editor.foreground": "#D4D4D4",
            "editorLineNumber.foreground": "#858585",
            "editor.lineHighlightBackground": "#2D2D30",
            "editorCursor.foreground": "#A7A7A7",
          }
        : {
            "editor.background": "#FFFFFF",
            "editor.foreground": "#000000",
            "editorLineNumber.foreground": "#237893",
            "editor.lineHighlightBackground": "#F3F3F3",
            "editorCursor.foreground": "#000000",
          },
    });
  };

  useEffect(() => {
    const handleVisibilityChange = () => {
      if (document.hidden) {
        localStorage.setItem(CODE_KEY, code || "");
        localStorage.setItem(MARKDOWN_KEY, markdown || "");
      }
    };

    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () => {
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, [code, markdown]);

  return (
    <div className="playground-container">
      <header className="playground-header">
        <div style={{ display: "flex", alignItems: "center" }}>
          <a
            href="https://harehare.github.io/mq/"
            style={{ textDecoration: "none", paddingTop: "4px" }}
            target="_blank"
          >
            <img src="./logo.svg" className="logo-icon" />
          </a>
          <h1>mq Playground</h1>
        </div>
        <a
          href="https://github.com/harehare/mq"
          style={{
            marginRight: "8px",
            textDecoration: "none",
            color: "inherit",
          }}
          target="_blank"
        >
          <FaGithub />
        </a>
      </header>

      <div className="playground-content">
        <div className="left-panel">
          <div className="editor-container">
            <div className="editor-header">
              <h2>Code</h2>
              <div className="editor-actions">
                <div>
                  <select
                    className="dropdown"
                    style={{
                      appearance: "none",
                      backgroundColor: isDarkMode ? "#2d2d30" : "#f5f5f5",
                      border: isDarkMode
                        ? "1px solid #3e3e42"
                        : "1px solid #ccc",
                      borderRadius: "4px",
                      padding: "6px 24px 6px 12px",
                      fontSize: "13px",
                      color: isDarkMode ? "#d4d4d4" : "#333",
                      cursor: "pointer",
                    }}
                    onChange={(e) => {
                      handleChangeExample(parseInt(e.target.value));
                    }}
                  >
                    {EXAMPLES.map((example, index) => (
                      <option key={index} value={index}>
                        {example.name}
                      </option>
                    ))}
                  </select>
                </div>
                <button
                  className="format-button"
                  onClick={handleFormat}
                  disabled={!wasmLoaded}
                >
                  Format
                </button>
                <button
                  className="run-button"
                  onClick={handleRun}
                  disabled={!wasmLoaded}
                >
                  ▶ Run
                </button>
              </div>
            </div>
            <div className="editor-content">
              <Editor
                className="editor"
                defaultLanguage="mq"
                value={code}
                onChange={setCode}
                beforeMount={beforeMount}
                options={{
                  minimap: { enabled: false },
                  scrollBeyondLastLine: false,
                  fontSize: 12,
                  automaticLayout: true,
                  fontFamily:
                    "'JetBrains Mono', 'Source Code Pro', Menlo, monospace",
                  fontLigatures: true,
                }}
                theme="mq-base"
              />
            </div>
          </div>

          <div className="editor-container">
            <div className="editor-header">
              <h2>Markdown</h2>
            </div>
            <div className="editor-content">
              <Editor
                height="100%"
                defaultLanguage="markdown"
                value={markdown}
                onChange={setMarkdown}
                options={{
                  minimap: { enabled: false },
                  scrollBeyondLastLine: false,
                  fontSize: 12,
                  automaticLayout: true,
                  fontFamily:
                    "'JetBrains Mono', 'Source Code Pro', Menlo, monospace",
                  fontLigatures: true,
                }}
                theme="mq-base"
              />
            </div>
          </div>
        </div>
        <div className="right-panel">
          <div className="editor-header">
            <h2>Output</h2>
          </div>
          <div className="editor-content result-container">
            {!wasmLoaded ? (
              <div className="loading-message">Loading WASM module...</div>
            ) : result ? (
              <Editor
                height="100%"
                defaultLanguage="markdown"
                value={result}
                options={{
                  readOnly: true,
                  domReadOnly: true,
                  minimap: { enabled: false },
                  scrollBeyondLastLine: false,
                  fontSize: 12,
                  automaticLayout: true,
                }}
                theme="mq-base"
              />
            ) : (
              <div className="empty-message">
                Click "Run" button to display results
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
