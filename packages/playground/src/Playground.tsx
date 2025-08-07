import { useState, useEffect, useCallback } from "react";
import Editor, { Monaco } from "@monaco-editor/react";
import "./index.css";
import * as mq from "mq-web";
import { FaGithub } from "react-icons/fa6";
import { languages } from "monaco-editor";
import LZString from "lz-string";

type SharedData = {
  code: string;
  markdown: string;
  options: mq.Options;
};

const CODE_KEY = "mq-playground.code";
const MARKDOWN_KEY = "mq-playground.markdown";
const IS_UPDATE_KEY = "mq-playground.is_update";
const INPUT_FORMAT_KEY = "mq-playground.input_format";
const EXAMPLES: {
  name: string;
  code: string;
  markdown: string;
  isUpdate: boolean;
  format: mq.Options["inputFormat"];
}[] = [
  {
    name: "Hello World",
    code: `# Hello world
select(.h || .code || .[])`,
    markdown: `# Hello world

- Hello
- world

\`\`\`
Hello world
\`\`\`
`,
    isUpdate: false,
    format: "markdown",
  },
  {
    name: "Update child node",
    code: `.h1 | get(1) | add("text")`,
    markdown: `# *h1* text

- item1
- item2
`,
    isUpdate: false,
    format: "markdown",
  },
  {
    name: "Markdown Toc",
    code: `.h
| let link = to_link("#" + to_text(self), to_text(self), "")
| let level = .h.depth
| if (not(is_none(level))): to_md_list(link, to_number(level))`,
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
    isUpdate: false,
    format: "markdown",
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
    isUpdate: false,
    format: "markdown",
  },
  {
    name: "Exclude js code",
    code: `select(!.code("js"))`,
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
    isUpdate: false,
    format: "markdown",
  },
  {
    name: "Extract mdx",
    code: `select(is_mdx())`,
    markdown: `import {Chart} from './snowfall.js'
import { isDarkMode } from '../../../textusm/frontend/src/ts/utils';
export const year = 2023

# Last year’s snowfall

In {year}, the snowfall was above average.

<Chart color="#fcb32c" year={year} />
`,
    isUpdate: false,
    format: "mdx",
  },
  {
    name: "Custom function",
    code: `def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
      let first_char = upcase(first(word))
      | let rest_str = downcase(word[1:len(word)])
      | s"\${first_char}\${rest_str}";
  | join("")
end
| snake_to_camel()`,
    markdown: `# sample_codes`,
    isUpdate: false,
    format: "markdown",
  },
  {
    name: "Generate sitemap",
    code: `def sitemap(item, base_url):
  let path = replace(to_text(item), ".md", ".html")
  | let loc = base_url + path
  | s"<url>
  <loc>\${loc}</loc>
  <priority>1.0</priority>
</url>"
end
| .[]
| sitemap("https://example.com/")`,
    markdown: `# Summary

- [Chapter1](chapter1.md)
- [Chapter2](Chapter2.md)
  - [Chapter3](Chapter3.md)
- [Chapter4](Chapter4.md)
`,
    isUpdate: false,
    format: "markdown",
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
    isUpdate: false,
    format: "markdown",
  },
  {
    name: "Extract list",
    code: `.[] | select(.list.level == "1")`,
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
    isUpdate: false,
    format: "markdown",
  },
  {
    name: "CSV to markdown table",
    code: `nodes | csv2table()`,
    markdown: `Product, Category, Price, Stock
---,---,---,---
Laptop, Electronics, $1200, 45
Monitor, Electronics, $350, 28
Chair, Furniture, $150, 73
Desk,  Furniture, $200, 14
Keyboard, Accessories, $80, 35
`,
    isUpdate: false,
    format: "text",
  },
];

export const Playground = () => {
  const [code, setCode] = useState<string | undefined>(
    localStorage.getItem(CODE_KEY) ?? EXAMPLES[0].code
  );
  const [markdown, setMarkdown] = useState<string | undefined>(
    localStorage.getItem(MARKDOWN_KEY) ?? EXAMPLES[0].markdown
  );
  const [isUpdate, setIsUpdate] = useState(
    localStorage.getItem(IS_UPDATE_KEY) === "true"
  );
  const [isEmbed, setIsEmbed] = useState(false);
  const [result, setResult] = useState("");
  const [listStyle, setListStyle] = useState<mq.Options["listStyle"]>(null);
  const [linkUrlStyle, setLinkUrlStyle] =
    useState<mq.Options["linkUrlStyle"]>(null);
  const [linkTitleStyle, setLinkTitleStyle] =
    useState<mq.Options["linkTitleStyle"]>(null);
  const [inputFormat, setInputFormat] = useState<mq.Options["inputFormat"]>(
    (() => {
      const format = localStorage.getItem(INPUT_FORMAT_KEY);
      return format === "markdown"
        ? "markdown"
        : format === "text"
        ? "text"
        : format === "mdx"
        ? "mdx"
        : format === "html"
        ? "html"
        : null;
    })()
  );
  const [activeTab, setActiveTab] = useState<"output" | "ast">("output");
  const [astResult, setAstResult] = useState("");

  useEffect(() => {
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
              isUpdate: !!options.isUpdate,
              inputFormat: options.inputFormat || null,
              listStyle: options.listStyle,
              linkUrlStyle: options.linkUrlStyle || null,
              linkTitleStyle: options.linkTitleStyle || null,
            },
          };
          setCode(data.code);
          setMarkdown(data.markdown);
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

    const urlParams = new URLSearchParams(window.location.search);
    const embedParam = urlParams.get("embed");
    setIsEmbed(embedParam === "true");

    const themeParam = urlParams.get("theme");
    if (themeParam) {
      document.documentElement.style.colorScheme =
        themeParam === "dark" ? "dark" : "light";
    }
  }, []);

  const handleRun = useCallback(async () => {
    if (!code || !markdown) {
      return;
    }
    setResult("Running...");
    setAstResult("");

    try {
      setResult(
        await mq.run(code, markdown, {
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
    isUpdate,
    listStyle,
    linkUrlStyle,
    linkTitleStyle,
  ]);

  const handleGenerateAst = useCallback(async () => {
    if (!code) {
      return;
    }
    setAstResult("Generating AST...");

    try {
      const ast = await mq.toAst(code);
      setAstResult(JSON.stringify(JSON.parse(ast), null, "  "));
    } catch (e) {
      setAstResult((e as Error).toString());
    }
  }, [code]);

  const handleFormat = useCallback(async () => {
    if (!code) {
      return;
    }

    setCode(await mq.format(code));
  }, [code]);

  const handleChangeExample = useCallback((index: number) => {
    const selected = EXAMPLES[index];
    setCode(selected.code);
    setMarkdown(selected.markdown);
    setIsUpdate(selected.isUpdate);
    setInputFormat(selected.format);
  }, []);

  const handleShare = useCallback(() => {
    const shareData: SharedData = {
      code: code || "",
      markdown: markdown || "",
      options: {
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
    isUpdate,
    listStyle,
    linkUrlStyle,
    linkTitleStyle,
  ]);

  const handleCopy = useCallback(() => {
    if (code) {
      const options = [
        isUpdate ? "-U" : "",
        inputFormat ? `-I ${inputFormat}` : "",
        listStyle ? `--list-style ${listStyle}` : "",
        linkUrlStyle ? `--link-url-style ${linkUrlStyle}` : "",
        linkTitleStyle ? `--link-title-style ${linkTitleStyle}` : "",
      ]
        .filter(Boolean)
        .join(" ");
      const script = `mq ${options} '${code}'`;

      navigator.clipboard.writeText(script).then(() => {
        alert("Command copied to clipboard!");
      });
    }
  }, [code, inputFormat, isUpdate, listStyle, linkUrlStyle, linkTitleStyle]);

  const handleChangeListStyle = useCallback(
    (e: React.ChangeEvent<HTMLSelectElement>) => {
      const value = e.target.value;
      setListStyle(value as mq.Options["listStyle"]);
    },
    []
  );

  const handleChangeLinkUrlStyle = useCallback(
    (e: React.ChangeEvent<HTMLSelectElement>) => {
      const value = e.target.value;
      setLinkUrlStyle(value as mq.Options["linkUrlStyle"]);
    },
    []
  );

  const beforeMount = (monaco: Monaco) => {
    const urlParams = new URLSearchParams(window.location.search);
    const themeParam = urlParams.get("theme");
    const isDarkMode = !themeParam
      ? window.matchMedia("(prefers-color-scheme: dark)").matches
      : themeParam === "dark";

    monaco.editor.addEditorAction({
      id: "run-script",
      label: "Run Script",
      keybindings: [monaco.KeyMod.WinCtrl | monaco.KeyCode.Enter],
      run: () => {
        handleRun();
      },
    });

    monaco.editor.onDidCreateEditor((editor) => {
      editor.onDidChangeModelContent(async () => {
        const model = editor.getModel();
        if (model) {
          const modelLanguage = model.getLanguageId();

          if (modelLanguage === "markdown") {
            return;
          }

          const errors = await mq.diagnostics(model.getValue());
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
      provideCompletionItems: async (model, position) => {
        const values = await mq.definedValues("");
        const wordRange = model.getWordUntilPosition(position);
        const suggestions: languages.CompletionItem[] = values.map((value) => {
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

        const snippets: languages.CompletionItem[] = [
          {
            label: "foreach",
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: "foreach (${1:item}, ${2:items}): ${0:body};",
            insertTextRules:
              monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: "Loop over each item in a collection",
            documentation:
              "Creates a foreach loop to iterate through items in a collection",
            range: {
              startLineNumber: position.lineNumber,
              startColumn: wordRange.startColumn,
              endLineNumber: position.lineNumber,
              endColumn: wordRange.endColumn,
            },
          },
          {
            label: "while",
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: "while (${1:condition}): ${0:body};",
            insertTextRules:
              monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: "Loop while condition is true",
            documentation:
              "Creates a while loop that continues execution as long as condition is true",
            range: {
              startLineNumber: position.lineNumber,
              startColumn: wordRange.startColumn,
              endLineNumber: position.lineNumber,
              endColumn: wordRange.endColumn,
            },
          },
          {
            label: "until",
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: "until (${1:condition}): ${0:body};",
            insertTextRules:
              monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: "Loop until condition is true",
            documentation:
              "Creates an until loop that continues execution until condition becomes true",
            range: {
              startLineNumber: position.lineNumber,
              startColumn: wordRange.startColumn,
              endLineNumber: position.lineNumber,
              endColumn: wordRange.endColumn,
            },
          },
          {
            label: "def",
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: "def ${0}(${1:args}): ${2:body};",
            insertTextRules:
              monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: "Define a custom function",
            documentation:
              "Creates a reusable function with custom parameters that can be called elsewhere in the script",
            range: {
              startLineNumber: position.lineNumber,
              startColumn: wordRange.startColumn,
              endLineNumber: position.lineNumber,
              endColumn: wordRange.endColumn,
            },
          },
          {
            label: "fn",
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: "fn(${0:args}): ${1:body};",
            insertTextRules:
              monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: "Define an anonymous function",
            documentation:
              "Creates an anonymous function that can be used inline without naming it",
            range: {
              startLineNumber: position.lineNumber,
              startColumn: wordRange.startColumn,
              endLineNumber: position.lineNumber,
              endColumn: wordRange.endColumn,
            },
          },
        ];

        return { suggestions: [...suggestions, ...snippets] };
      },
    });
    monaco.languages.register({ id: "mq" });
    monaco.languages.setMonarchTokensProvider("mq", {
      tokenizer: {
        root: [
          [/#[^\n]*/, "comment"],
          [
            /\b(let|def|while|foreach|until|if|elif|else|end|self|None|nodes|break|continue)\b/,
            "keyword",
          ],
          [/;/, "delimiter"],
          [/(->|=|\||:|;|\?|!|\+|-|\*|\/|%|<=|>=|==|!=|<|>|&&)/, "operator"],
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
          [/\\./, "string.escape"], // handle escaped characters (including \")
          [/[^\\"]+/, "string"], // match all except backslash and quote
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
        localStorage.setItem(IS_UPDATE_KEY, String(isUpdate));
        localStorage.setItem(INPUT_FORMAT_KEY, inputFormat || "markdown");
      }
    };

    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () => {
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, [code, markdown, isUpdate, inputFormat]);

  return (
    <div className="playground-container">
      {!isEmbed && (
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
      )}

      <div className="playground-content">
        <div className="left-panel">
          <div className="editor-container">
            <div className="editor-header code">
              <h2>Code</h2>
              <div className="editor-actions">
                <div>
                  <select
                    className="dropdown"
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
                <button className="button" onClick={handleCopy}>
                  Copy
                </button>
                <button className="button" onClick={handleShare}>
                  Share
                </button>
                <button className="button format-button" onClick={handleFormat}>
                  Format
                </button>
                <button className="button run-button" onClick={handleRun}>
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
                  value={inputFormat || "markdown"}
                  onChange={(e) =>
                    setInputFormat(e.target.value as mq.Options["inputFormat"])
                  }
                >
                  <option value="markdown">Markdown</option>
                  <option value="mdx">MDX</option>
                  <option value="text">Text</option>
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
            <div className="tab-container">
              <button
                className={`tab ${activeTab === "output" ? "active" : ""}`}
                onClick={() => setActiveTab("output")}
              >
                Output
              </button>
              <button
                className={`tab ${activeTab === "ast" ? "active" : ""}`}
                onClick={() => setActiveTab("ast")}
              >
                AST
              </button>
            </div>
            {!isEmbed && (
              <div className="editor-actions">
                {activeTab === "output" && (
                  <>
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
                        onChange={(e) => {
                          const value = e.target.value;
                          const linkTitleStyle =
                            value === "none"
                              ? null
                              : (value as mq.Options["linkTitleStyle"]);
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
                  </>
                )}
                {activeTab === "ast" && (
                  <button className="button" onClick={handleGenerateAst}>
                    Generate AST
                  </button>
                )}
              </div>
            )}
          </div>
          <div className="editor-content result-container">
            {activeTab === "output" && (
              <Editor
                height="100%"
                defaultLanguage="markdown"
                defaultValue={`Click "Run" button to display results`}
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
            )}
            {activeTab === "ast" && (
              <Editor
                height="100%"
                defaultLanguage="json"
                defaultValue={`Click "Generate AST" button to display AST`}
                value={astResult}
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
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
