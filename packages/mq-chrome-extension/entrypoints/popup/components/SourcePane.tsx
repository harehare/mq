import { CodeEditor } from "./CodeEditor";

type SourcePaneProps = {
  markdown: string;
  isExtracting: boolean;
  onMarkdownChange: (value: string) => void;
  onExtract: () => void;
};

export function SourcePane({
  markdown,
  isExtracting,
  onMarkdownChange,
  onExtract,
}: SourcePaneProps) {
  return (
    <section className="pane pane-source">
      <div className="pane-header">
        <h2>Source</h2>
        <button
          type="button"
          className="primary"
          onClick={onExtract}
          disabled={isExtracting}
        >
          {isExtracting ? "Extracting…" : "Extract page"}
        </button>
      </div>
      <CodeEditor
        className="pane-textarea"
        language="markdown"
        placeholder="Click “Extract page” to convert the current page to Markdown, or paste your own."
        value={markdown}
        onChange={onMarkdownChange}
      />
    </section>
  );
}
