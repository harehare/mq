import { EXAMPLE_QUERIES } from "../lib/examples";

type QueryEditorProps = {
  query: string;
  isRunning: boolean;
  disabled: boolean;
  onChange: (value: string) => void;
  onRun: () => void;
};

export function QueryEditor({
  query,
  isRunning,
  disabled,
  onChange,
  onRun,
}: QueryEditorProps) {
  return (
    <section className="pane">
      <div className="pane-header">
        <h2>mq Query</h2>
        <select
          aria-label="Example queries"
          value=""
          onChange={(event) => {
            if (event.target.value) onChange(event.target.value);
          }}
        >
          <option value="">Examples…</option>
          {EXAMPLE_QUERIES.map((example) => (
            <option key={example.name} value={example.code}>
              {example.name}
            </option>
          ))}
        </select>
      </div>
      <textarea
        className="pane-textarea query-textarea"
        placeholder=".h"
        value={query}
        onChange={(event) => onChange(event.target.value)}
        spellCheck={false}
      />
      <button
        type="button"
        className="run-button"
        onClick={onRun}
        disabled={disabled || isRunning}
      >
        {isRunning ? "Running…" : "Run"}
      </button>
    </section>
  );
}
