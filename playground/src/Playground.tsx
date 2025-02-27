import { useState, useEffect, useRef, useCallback } from "react";
import Editor, { Monaco } from "@monaco-editor/react";
import "./index.css";

import init, { Script } from "./mq-wasm/mq_wasm";
import { editor } from "monaco-editor";

export const Playground = () => {
  const [code, setCode] = useState<string | undefined>(`# Sample
def hello_world():
  add(" Hello World");
.[] | upcase()? | hello_world()?
`);

  const [markdown, setMarkdown] = useState<
    string | undefined
  >(`# Sample Markdown

- Hello
- World

\`\`\`
Code block
\`\`\`
`);

  const [result, setResult] = useState("");
  const [wasmLoaded, setWasmLoaded] = useState(false);

  const scriptRef = useRef<Script>(null);
  const codeRef = useRef<editor.IStandaloneCodeEditor>(null);
  const markdownRef = useRef<editor.IStandaloneCodeEditor>(null);

  useEffect(() => {
    init().then(() => {
      scriptRef.current = Script.new();
      setWasmLoaded(true);
    });
  }, []);

  const handleCodeEditorDidMount = (editor: editor.IStandaloneCodeEditor) => {
    codeRef.current = editor;
  };

  const handleMarkdownEditorDidMount = (
    editor: editor.IStandaloneCodeEditor
  ) => {
    markdownRef.current = editor;
  };

  const run = useCallback(async () => {
    if (!scriptRef.current || !code || !markdown) {
      return;
    }

    try {
      setResult(await scriptRef.current.run(code, markdown));
    } catch (e) {
      setResult((e as Error).toString());
    }
  }, [code, markdown]);

  const format = useCallback(async () => {
    if (!scriptRef.current || !code) {
      return;
    }

    setCode(await scriptRef.current.format(code));
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
          [/(([a-zA-Z_]\w*)\s*\()/, "function"],
          [/\(|\)|\[|\]/, "delimiter.parenthesis"],
          [/[a-zA-Z_]\w*/, "identifier"],
        ],
      },
    });

    monaco.editor.defineTheme("mq-base", {
      base: "vs-dark",
      inherit: true,
      rules: [
        { token: "comment", foreground: "#6A9955", fontStyle: "italic" },
        { token: "keyword", foreground: "#569CD6", fontStyle: "bold" },
        { token: "function", foreground: "#DCDCAA" },
        { token: "variable", foreground: "#9CDCFE" },
        { token: "property", foreground: "#9CDCFE" },
        { token: "string", foreground: "#CE9178" },
        { token: "number", foreground: "#B5CEA8" },
        { token: "operator", foreground: "#D4D4D4" },
        { token: "delimiter", foreground: "#D4D4D4" },
        { token: "identifier", foreground: "#D4D4D4" },
      ],
      colors: {
        "editor.background": "#1E1E1E",
        "editor.foreground": "#D4D4D4",
        "editorLineNumber.foreground": "#858585",
        "editor.lineHighlightBackground": "#2D2D30",
        "editorCursor.foreground": "#A7A7A7",
      },
    });
  };

  return (
    <div className="playground-container">
      <header className="playground-header">
        <img src="../public/logo.svg" className="logo-icon" />
        <h1>mq Playground</h1>
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
                  Run
                </button>
              </div>
            </div>
            <div className="editor-content">
              <Editor
                height="100%"
                defaultLanguage="mq"
                value={code}
                onChange={setCode}
                onMount={handleCodeEditorDidMount}
                beforeMount={beforeMount}
                options={{
                  minimap: { enabled: false },
                  scrollBeyondLastLine: false,
                  fontSize: 12,
                  automaticLayout: true,
                }}
                theme="vs-dark"
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
                onMount={handleMarkdownEditorDidMount}
                options={{
                  minimap: { enabled: false },
                  scrollBeyondLastLine: false,
                  fontSize: 14,
                  automaticLayout: true,
                }}
                theme="vs-light"
              />
            </div>
          </div>
        </div>
        <div className="right-panel">
          <div className="editor-header">
            <h2>Execution Result</h2>
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
                theme="vs-light"
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
