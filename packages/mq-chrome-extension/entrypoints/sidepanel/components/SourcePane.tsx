import { CodeEditor } from "./CodeEditor";

export type SourceMode = "markdown" | "html";

type SourcePaneProps = {
  mode: SourceMode;
  markdown: string;
  html: string;
  isExtracting: boolean;
  onModeChange: (mode: SourceMode) => void;
  onMarkdownChange: (value: string) => void;
  onHtmlChange: (value: string) => void;
  onExtract: () => void;
};

export function SourcePane({
  mode,
  markdown,
  html,
  isExtracting,
  onModeChange,
  onMarkdownChange,
  onHtmlChange,
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
      <div className="tabs" role="tablist" aria-label="Source format">
        <button
          type="button"
          role="tab"
          aria-selected={mode === "markdown"}
          className={`tab ${mode === "markdown" ? "active" : ""}`}
          onClick={() => onModeChange("markdown")}
        >
          Markdown
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={mode === "html"}
          className={`tab ${mode === "html" ? "active" : ""}`}
          onClick={() => onModeChange("html")}
        >
          HTML
        </button>
      </div>
      {mode === "markdown" ? (
        <CodeEditor
          className="pane-textarea"
          language="markdown"
          placeholder="Click “Extract page” to convert the current page to Markdown, or paste your own."
          value={markdown}
          onChange={onMarkdownChange}
        />
      ) : (
        <CodeEditor
          className="pane-textarea"
          language="html"
          placeholder="Click “Extract page” to grab the current page's raw HTML, or paste your own. Queries run directly against it."
          value={html}
          onChange={onHtmlChange}
        />
      )}
    </section>
  );
}
