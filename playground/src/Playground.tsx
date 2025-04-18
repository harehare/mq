import { useState, useEffect, useCallback } from "react";
import Editor, { Monaco } from "@monaco-editor/react";
import "./index.css";
import init, {
  runScript,
  formatScript,
  definedValues,
  diagnostics,
  RunOptions,
} from "./mq-wasm/mq_wasm";
import { FaGithub } from "react-icons/fa6";
import { languages } from "monaco-editor";
import LZString from "lz-string";

type SharedData = {
  code: string;
  markdown: string;
  options: RunOptions;
};

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
    isMdx: false,
    isUpdate: false,
  },
  {
    name: "Update child node",
    code: `.h1 | nth(1) | add("text")`,
    markdown: `# *h1* text

- item1
- item2
`,
    isMdx: false,
    isUpdate: false,
  },
  {
    name: "Markdown Toc",
    code: `.h
| let link = to_link(add("#", to_text(self)), to_text(self), "")
| if (eq(to_md_name(), "h1")):
  to_md_list(link, 1)
elif (eq(to_md_name(), "h2")):
  to_md_list(link, 2)
elif (eq(to_md_name(), "h3")):
  to_md_list(link, 3)
elif (eq(to_md_name(), "h4")):
  to_md_list(link, 4)
elif (eq(to_md_name(), "h5")):
  to_md_list(link, 5)
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
    isMdx: false,
    isUpdate: false,
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
    isMdx: false,
    isUpdate: false,
  },
  {
    name: "Exclude js code",
    code: `select(not(.code("js")))`,
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
    isMdx: false,
    isUpdate: false,
  },
  {
    name: "Extract mdx",
    code: `select(is_mdx())`,
    markdown: `import {Chart} from './snowfall.js'
export const year = 2023

# Last year’s snowfall

In {year}, the snowfall was above average.

<Chart color="#fcb32c" year={year} />
`,
    isMdx: true,
    isUpdate: false,
  },
  {
    name: "Custom function",
    code: `def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
      let first_char = upcase(first(word))
      | let rest_str = downcase(slice(word, 1, len(word)))
      | s"\${first_char}\${rest_str}";
  | join("");
| snake_to_camel()`,
    markdown: `# sample_codes`,
    isMdx: false,
    isUpdate: false,
  },
  {
    name: "Generate sitemap",
    code: `def sitemap(item, base_url):
  let path = replace(to_text(item), ".md", ".html")
  | let loc = add(base_url, path)
  | s"<url>
  <loc>\${loc}</loc>
  <priority>1.0</priority>
</url>";
| .[]
| sitemap("https://example.com/")`,
    markdown: `# Summary

- [Chapter1](chapter1.md)
- [Chapter2](Chapter2.md)
  - [Chapter3](Chapter3.md)
- [Chapter4](Chapter4.md)
`,
    isMdx: false,
    isUpdate: false,
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
    isMdx: false,
    isUpdate: false,
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
    isMdx: false,
    isUpdate: false,
  },
];

export const Playground = () => {
  const [code, setCode] = useState<string | undefined>(
    localStorage.getItem(CODE_KEY) || EXAMPLES[0].code
  );
  const [markdown, setMarkdown] = useState<string | undefined>(
    localStorage.getItem(MARKDOWN_KEY) || EXAMPLES[0].markdown
  );
  const [isMdx, setIsMdx] = useState(false);
  const [isUpdate, setIsUpdate] = useState(true);
  const [result, setResult] = useState("");
  const [wasmLoaded, setWasmLoaded] = useState(false);
  const [listStyle, setListStyle] = useState<RunOptions["listStyle"]>(null);
  const [linkUrlStyle, setLinkUrlStyle] =
    useState<RunOptions["linkUrlStyle"]>(null);
  const [linkTitleStyle, setLinkTitleStyle] =
    useState<RunOptions["linkTitleStyle"]>(null);
  const [inputFormat, setInputFormat] =
    useState<RunOptions["inputFormat"]>(null);

  useEffect(() => {
    init().then(() => {
      setWasmLoaded(true);
    });

    if (window.location.hash) {
      try {
        const compressed = window.location.hash.substring(1);
        const decompressed =
          LZString.decompressFromEncodedURIComponent(compressed);
        if (decompressed) {
          const parsedData = JSON.parse(decompressed);
          const options = parsedData.options || {};
          const data: SharedData = {
            code: typeof parsedData.code === "string" ? parsedData.code : "",
            markdown:
              typeof parsedData.markdown === "string"
                ? parsedData.markdown
                : "",
            options: {
              isMdx: !!options.isMdx,
              isUpdate: !!options.isUpdate,
              inputFormat: options.inputFormat || null,
              listStyle: options.listStyle,
              linkUrlStyle: options.linkUrlStyle || null,
              linkTitleStyle: options.linkTitleStyle || null,
            },
          };
          setCode(data.code);
          setMarkdown(data.markdown);
          setIsMdx(data.options.isMdx === true);
          setIsUpdate(data.options.isUpdate === true);
          setInputFormat(data.options.inputFormat);
          setListStyle(data.options.listStyle);
          setLinkUrlStyle(data.options.linkUrlStyle);
          setLinkTitleStyle(data.options.linkTitleStyle);
        }
      } catch {
        alert("Failed to load shared playground");
      }
    }
  }, []);

  const handleRun = useCallback(async () => {
    if (!code || !markdown) {
      return;
    }

    try {
      setResult(
        runScript(code, markdown, {
          isMdx,
          isUpdate,
          inputFormat,
          listStyle,
          linkTitleStyle,
          linkUrlStyle,
        })
      );
    } catch (e) {
      setResult((e as Error).toString());
    }
  }, [
    code,
    markdown,
    inputFormat,
    isMdx,
    isUpdate,
    listStyle,
    linkUrlStyle,
    linkTitleStyle,
  ]);

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
    setIsMdx(selected.isMdx);
    setIsUpdate(selected.isUpdate);
    setInputFormat("markdown");
  }, []);

  const handleShare = useCallback(() => {
    const shareData: SharedData = {
      code: code || "",
      markdown: markdown || "",
      options: {
        isMdx: isMdx || false,
        isUpdate: isUpdate || false,
        inputFormat: inputFormat || null,
        listStyle: listStyle || null,
        linkUrlStyle: linkUrlStyle || null,
        linkTitleStyle: linkTitleStyle || null,
      },
    };
    const compressed = LZString.compressToEncodedURIComponent(
      JSON.stringify(shareData)
    );
    const url = `${window.location.origin}${window.location.pathname}#${compressed}`;
    window.location.hash = compressed;

    navigator.clipboard
      .writeText(url)
      .then(() => {
        alert("Share URL copied to clipboard!");
      })
      .catch(() => {
        prompt("Copy this URL to share your playground:", url);
      });
  }, [
    code,
    markdown,
    inputFormat,
    isMdx,
    isUpdate,
    listStyle,
    linkUrlStyle,
    linkTitleStyle,
  ]);

  const handleChangeListStyle = useCallback(
    (e: React.ChangeEvent<HTMLSelectElement>) => {
      const value = e.target.value;
      setListStyle(value as RunOptions["listStyle"]);
    },
    []
  );

  const handleChangeLinkUrlStyle = useCallback(
    (e: React.ChangeEvent<HTMLSelectElement>) => {
      const value = e.target.value;
      setLinkUrlStyle(value as RunOptions["linkUrlStyle"]);
    },
    []
  );

  const beforeMount = (monaco: Monaco) => {
    monaco.editor.addEditorAction({
      id: "run-script",
      label: "Run Script",
      keybindings: [monaco.KeyMod.WinCtrl | monaco.KeyCode.Enter],
      run: () => {
        handleRun();
      },
    });

    monaco.editor.onDidCreateEditor((editor) => {
      editor.onDidChangeModelContent(() => {
        const model = editor.getModel();
        if (model) {
          const modelLanguage = model.getLanguageId();

          if (modelLanguage === "markdown") {
            return;
          }

          const errors = diagnostics(model.getValue());
          monaco.editor.setModelMarkers(
            model,
            "mq",
            errors.map((error) => ({
              startLineNumber: error.startLine,
              startColumn: error.startColumn,
              endLineNumber: error.endLine,
              endColumn: error.endColumn,
              message: error.message,
              severity: monaco.MarkerSeverity.Error,
            }))
          );
        }
      });
    });

    monaco.languages.registerCompletionItemProvider("mq", {
      triggerCharacters: [" ", "|"],
      provideCompletionItems: (model, position) => {
        const values = definedValues("");
        const suggestions: languages.CompletionItem[] = values.map((value) => {
          const wordRange = model.getWordUntilPosition(position);
          return {
            label: value.name,
            kind:
              value.valueType === "Function"
                ? monaco.languages.CompletionItemKind.Function
                : value.valueType === "Variable"
                ? monaco.languages.CompletionItemKind.Variable
                : value.valueType === "Selector"
                ? monaco.languages.CompletionItemKind.Method
                : monaco.languages.CompletionItemKind.Property,
            insertText:
              value.valueType === "Function"
                ? `${value.name}(${
                    value.args
                      ?.map((name, i) => `$\{${i}:${name}}`)
                      .join(", ") || ""
                  })`
                : value.name,
            insertTextRules:
              monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: value.doc,
            documentation: value.doc,
            range: {
              startLineNumber: position.lineNumber,
              startColumn: wordRange.startColumn,
              endLineNumber: position.lineNumber,
              endColumn: wordRange.endColumn,
            },
          };
        });

        return { suggestions };
      },
    });
    monaco.languages.register({ id: "mq" });
    monaco.languages.setMonarchTokensProvider("mq", {
      tokenizer: {
        root: [
          [/^#.*$/, "comment"],
          [
            /\b(let|def|while|foreach|until|if|elif|else|self|None)\b/,
            "keyword",
          ],
          [/;/, "delimiter"],
          [/\|/, "operator"],
          [/"/, { token: "string", next: "@multilineString" }],
          [/"s"/, { token: "string", next: "@multilineString" }],
          [/\d+/, "number"],
          [/[a-zA-Z_]\w*(?=\s*\()/, "function"],
          [/(([a-zA-Z_]\w*)\s*\()/, "function"],
          [/\(|\)|\[|\]/, "delimiter.parenthesis"],
          [/[a-zA-Z_]\w*/, "variable"],
        ],
        multilineString: [
          [/\$\{[^}]*\}/, "variable"],
          [/[^"$]+/, "string"],
          [/\$(?!\{)/, "string"],
          [/\n/, "string"],
          [/"/, { token: "string", next: "@pop" }],
        ],
      },
      unicode: true,
      includeLF: true,
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
            href="https://mqlang.org/"
            style={{ textDecoration: "none", paddingTop: "4px" }}
            target="_blank"
          >
            <img src="./logo.svg" className="logo-icon" />
          </a>
          <h1>Playground</h1>
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
                <button className="share-button" onClick={handleShare}>
                  Share
                </button>
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
              <label className="label">
                <select
                  className="dropdown"
                  style={{
                    backgroundColor: isDarkMode ? "#2d2d30" : "#f5f5f5",
                    border: isDarkMode ? "1px solid #3e3e42" : "1px solid #ccc",
                    color: isDarkMode ? "#d4d4d4" : "#333",
                  }}
                  value={inputFormat || "markdown"}
                  onChange={(e) =>
                    setInputFormat(e.target.value as RunOptions["inputFormat"])
                  }
                >
                  <option value="markdown">Markdown</option>
                  <option value="html">HTML</option>
                </select>
              </label>
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
          <div className="editor-header output">
            <h2>Output</h2>
            <div className="editor-actions">
              <label className="label">
                <div
                  style={{
                    marginRight: "4px",
                  }}
                >
                  List Style:
                </div>
                <select
                  className="dropdown"
                  style={{
                    backgroundColor: isDarkMode ? "#2d2d30" : "#f5f5f5",
                    border: isDarkMode ? "1px solid #3e3e42" : "1px solid #ccc",
                    color: isDarkMode ? "#d4d4d4" : "#333",
                  }}
                  onChange={handleChangeListStyle}
                >
                  <option value="dash">-</option>
                  <option value="star">*</option>
                  <option value="plus">+</option>
                </select>
              </label>
              <label className="label">
                <div
                  style={{
                    marginRight: "4px",
                  }}
                >
                  URL Style:
                </div>
                <select
                  className="dropdown"
                  style={{
                    backgroundColor: isDarkMode ? "#2d2d30" : "#f5f5f5",
                    border: isDarkMode ? "1px solid #3e3e42" : "1px solid #ccc",
                    color: isDarkMode ? "#d4d4d4" : "#333",
                  }}
                  onChange={handleChangeLinkUrlStyle}
                >
                  <option value="none">None</option>
                  <option value="angle">Angle</option>
                </select>
              </label>
              <label className="label">
                <div
                  style={{
                    marginRight: "4px",
                  }}
                >
                  Title Style:
                </div>
                <select
                  className="dropdown"
                  style={{
                    backgroundColor: isDarkMode ? "#2d2d30" : "#f5f5f5",
                    border: isDarkMode ? "1px solid #3e3e42" : "1px solid #ccc",
                    color: isDarkMode ? "#d4d4d4" : "#333",
                  }}
                  onChange={(e) => {
                    const value = e.target.value;
                    const linkTitleStyle =
                      value === "none"
                        ? null
                        : (value as RunOptions["linkTitleStyle"]);
                    setLinkTitleStyle(linkTitleStyle);
                  }}
                >
                  <option value="none">None</option>
                  <option value="double">Double</option>
                  <option value="single">Single</option>
                  <option value="paren">Paren</option>
                </select>
              </label>
              <div>
                <label className="label">
                  <input
                    type="checkbox"
                    checked={isMdx}
                    onChange={(e) => setIsMdx(e.target.checked)}
                    style={{
                      marginRight: "5px",
                      cursor: "pointer",
                    }}
                  />
                  <div>Enable MDX</div>
                </label>
              </div>
              <div>
                <label
                  style={{
                    marginLeft: "4px",
                    display: "flex",
                    alignItems: "center",
                    fontSize: "13px",
                    cursor: "pointer",
                    userSelect: "none",
                  }}
                >
                  <input
                    type="checkbox"
                    checked={isUpdate}
                    onChange={(e) => setIsUpdate(e.target.checked)}
                    style={{
                      marginRight: "5px",
                      cursor: "pointer",
                    }}
                  />
                  <div>Update Markdown</div>
                </label>
              </div>
            </div>
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
