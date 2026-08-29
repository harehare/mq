import { CodeEditor } from "./CodeEditor";

type ResultPaneProps = {
  result: string;
  copied: boolean;
  lastRun: { durationMs: number } | null;
  onCopy: () => void;
  onDownload: () => void;
};

export function ResultPane({
  result,
  copied,
  lastRun,
  onCopy,
  onDownload,
}: ResultPaneProps) {
  return (
    <section className="pane pane-result">
      <div className="pane-header">
        <h2>Result</h2>
        <div className="pane-actions">
          <button
            type="button"
            className={copied ? "copied" : ""}
            onClick={onCopy}
            disabled={!result}
          >
            {copied ? "Copied!" : "Copy"}
          </button>
          <button type="button" onClick={onDownload} disabled={!result}>
            Download
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
      {lastRun && (
        <div className="result-meta">
          {result.length.toLocaleString()} characters · {lastRun.durationMs} ms
        </div>
      )}
    </section>
  );
}
