type ResultPaneProps = {
  result: string;
  previewActive: boolean;
  previewDisabled: boolean;
  onCopy: () => void;
  onTogglePreview: () => void;
};

export function ResultPane({
  result,
  previewActive,
  previewDisabled,
  onCopy,
  onTogglePreview,
}: ResultPaneProps) {
  return (
    <section className="pane">
      <div className="pane-header">
        <h2>Result</h2>
        <div className="button-group">
          <button type="button" onClick={onCopy} disabled={!result}>
            Copy
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
      <textarea
        className="pane-textarea"
        readOnly
        value={result}
        placeholder="Run a query to see the filtered Markdown here."
        spellCheck={false}
      />
    </section>
  );
}
