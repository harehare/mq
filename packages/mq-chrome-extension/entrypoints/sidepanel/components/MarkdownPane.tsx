type MarkdownPaneProps = {
  markdown: string;
  isExtracting: boolean;
  onChange: (value: string) => void;
  onExtract: () => void;
};

export function MarkdownPane({
  markdown,
  isExtracting,
  onChange,
  onExtract,
}: MarkdownPaneProps) {
  return (
    <section className="pane">
      <div className="pane-header">
        <h2>Page Markdown</h2>
        <button type="button" onClick={onExtract} disabled={isExtracting}>
          {isExtracting ? "Extracting…" : "Extract page"}
        </button>
      </div>
      <textarea
        className="pane-textarea"
        placeholder="Click “Extract page” to convert the current page to Markdown."
        value={markdown}
        onChange={(event) => onChange(event.target.value)}
        spellCheck={false}
      />
    </section>
  );
}
