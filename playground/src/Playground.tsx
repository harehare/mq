import { useState, useEffect, useCallback } from "react";
import Editor, { Monaco } from "@monaco-editor/react";
import "./index.css";

import init, { runScript, formatScript } from "./mq-wasm/mq_wasm";
import { FaGithub } from "react-icons/fa6";

const CODE_KEY = "mq-playground.code";
const MARKDOWN_KEY = "mq-playground.markdown";
const isDarkMode = window.matchMedia("(prefers-color-scheme: dark)").matches;

export const Playground = () => {
  const [code, setCode] = useState<string | undefined>(
    localStorage.getItem(CODE_KEY) ||
      `# Sample
def hello_world():
  add(" Hello World")?;
select(or(.[], .code, .h)) | upcase() | hello_world()
`
  );

  const [markdown, setMarkdown] = useState<string | undefined>(
    localStorage.getItem(MARKDOWN_KEY) ||
      `# Sample Markdown

- Hello
- World

\`\`\`
Code block
\`\`\`
`
  );

  const [result, setResult] = useState("");
  const [wasmLoaded, setWasmLoaded] = useState(false);

  useEffect(() => {
    init().then(() => {
      setWasmLoaded(true);
    });
  }, []);

  const run = useCallback(async () => {
    if (!code || !markdown) {
      return;
    }

    try {
      setResult(await runScript(code, markdown));
    } catch (e) {
      setResult((e as Error).toString());
    }
  }, [code, markdown]);

  const format = useCallback(async () => {
    if (!code) {
      return;
    }

    setCode(await formatScript(code));
  }, [code]);

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
              <div className="editor-buttons">
                <button
                  className="format-button"
                  onClick={format}
                  disabled={!wasmLoaded}
                >
                  Format
                </button>
                <button
                  className="run-button"
                  onClick={run}
                  disabled={!wasmLoaded}
                >
                  ▶ Run
                </button>
              </div>
            </div>
            <div className="editor-content">
              <Editor
                height="100%"
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
