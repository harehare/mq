import { CodeEditor } from "./CodeEditor";

type ResultPaneProps = {
  result: string;
  previewActive: boolean;
  previewDisabled: boolean;
  copied: boolean;
  onCopy: () => void;
  onTogglePreview: () => void;
};

export function ResultPane({
  result,
  previewActive,
  previewDisabled,
  copied,
  onCopy,
  onTogglePreview,
}: ResultPaneProps) {
  return (
    <section className="pane pane-result">
      <div className="pane-header">
        <h2>Result</h2>
        <div className="button-group">
          <button
            type="button"
            className={copied ? "copied" : ""}
            onClick={onCopy}
            disabled={!result}
          >
            {copied ? "Copied!" : "Copy"}
          </button>
          <button
            type="button"
            onClick={onTogglePreview}
            disabled={previewDisabled}
            className={previewActive ? "active" : ""}
          >
            {previewActive ? "Exit preview" : "Preview on page"}
          </button>
        </div>
      </div>
      <CodeEditor
        className="pane-textarea"
        language="markdown"
        readOnly
        value={result}
        placeholder="Run a query to see the filtered Markdown here."
      />
    </section>
  );
}
