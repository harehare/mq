import CodeMirror, { EditorView } from "@uiw/react-codemirror";
import { syntaxHighlighting } from "@codemirror/language";
import { markdown } from "@codemirror/lang-markdown";
import { html } from "@codemirror/lang-html";
import { mqLanguage } from "../lib/mqLanguage";
import { editorHighlightStyle } from "../lib/editorHighlight";

export type EditorLanguage = "markdown" | "html" | "mq";

type CodeEditorProps = {
  value: string;
  language: EditorLanguage;
  placeholder?: string;
  readOnly?: boolean;
  highlightActiveLine?: boolean;
  className?: string;
  onChange?: (value: string) => void;
};

const languageExtension = (language: EditorLanguage) => {
  switch (language) {
    case "markdown":
      return markdown();
    case "html":
      return html();
    case "mq":
      return mqLanguage;
  }
};

const editorTheme = EditorView.theme({
  "&": {
    height: "100%",
    width: "100%",
    backgroundColor: "var(--panel-bg)",
    color: "var(--fg)",
    fontSize: "12px",
  },
  ".cm-content": {
    fontFamily: '"JetBrains Mono", monospace',
    padding: "10px",
    caretColor: "var(--fg)",
  },
  ".cm-scroller": {
    fontFamily: '"JetBrains Mono", monospace',
    lineHeight: "1.5",
  },
  ".cm-gutters": {
    display: "none",
  },
  ".cm-activeLine": {
    backgroundColor: "color-mix(in srgb, var(--header-title-color) 8%, transparent)",
  },
  "&.cm-focused": {
    outline: "none",
  },
  ".cm-placeholder": {
    color: "var(--muted)",
  },
  ".cm-selectionBackground": {
    backgroundColor: "color-mix(in srgb, var(--header-title-color) 25%, transparent) !important",
  },
  ".cm-cursor, .cm-dropCursor": {
    borderLeftColor: "var(--fg)",
  },
});

export function CodeEditor({
  value,
  language,
  placeholder,
  readOnly,
  highlightActiveLine = !readOnly,
  className,
  onChange,
}: CodeEditorProps) {
  return (
    <CodeMirror
      className={className}
      value={value}
      height="100%"
      placeholder={placeholder}
      editable={!readOnly}
      basicSetup={{
        lineNumbers: false,
        foldGutter: false,
        highlightActiveLine,
        highlightActiveLineGutter: false,
      }}
      theme={editorTheme}
      extensions={[languageExtension(language), syntaxHighlighting(editorHighlightStyle)]}
      onChange={onChange}
    />
  );
}
