import { CodeEditor } from "./CodeEditor";

type ResultPaneProps = {
  result: string;
  copied: boolean;
  onCopy: () => void;
};

export function ResultPane({ result, copied, onCopy }: ResultPaneProps) {
  return (
    <section className="pane pane-result">
      <div className="pane-header">
        <h2>Result</h2>
        <button
          type="button"
          className={copied ? "copied" : ""}
          onClick={onCopy}
          disabled={!result}
        >
          {copied ? "Copied!" : "Copy"}
        </button>
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
