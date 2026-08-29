import { EXAMPLE_CATEGORIES } from "../lib/examples";
import { isRunShortcut } from "../lib/shortcuts";
import { CodeEditor } from "./CodeEditor";

type QueryEditorProps = {
  query: string;
  isRunning: boolean;
  disabled: boolean;
  onChange: (value: string) => void;
  onSelectExample: (code: string) => void;
  onRun: () => void;
};

export function QueryEditor({
  query,
  isRunning,
  disabled,
  onChange,
  onSelectExample,
  onRun,
}: QueryEditorProps) {
  return (
    <section className="pane pane-query">
      <div className="pane-header">
        <h2>mq Query</h2>
        <select
          aria-label="Example queries"
          title="Pick an example to load it and run it immediately"
          value=""
          onChange={(event) => {
            if (event.target.value) onSelectExample(event.target.value);
          }}
        >
          <option value="">Examples…</option>
          {EXAMPLE_CATEGORIES.map((category) => (
            <optgroup key={category.name} label={category.name}>
              {category.examples.map((example) => (
                <option key={example.name} value={example.code}>
                  {example.name}
                </option>
              ))}
            </optgroup>
          ))}
        </select>
      </div>
      <CodeEditor
        className="pane-textarea query-textarea"
        language="mq"
        placeholder=".h"
        value={query}
        highlightActiveLine={false}
        onChange={onChange}
        onKeyDown={(event) => {
          if (isRunShortcut(event)) {
            event.preventDefault();
            if (!disabled && !isRunning) onRun();
          }
        }}
      />
      <span className="keyboard-hint">
        {isRunning ? "Running…" : "Auto-run enabled · ⌘/Ctrl + Enter to run now"}
      </span>
    </section>
  );
}
