import type { Options } from "../lib/mq";

export type RunOptions = Pick<
  Options,
  "listStyle" | "linkUrlStyle" | "linkTitleStyle"
>;

type OptionsPanelProps = {
  options: RunOptions;
  onChange: (options: RunOptions) => void;
};

export function OptionsPanel({ options, onChange }: OptionsPanelProps) {
  return (
    <details className="options-panel">
      <summary>Options</summary>
      <div className="options-grid">
        <label>
          List style
          <select
            value={options.listStyle ?? "dash"}
            onChange={(event) =>
              onChange({
                ...options,
                listStyle: event.target.value as Options["listStyle"],
              })
            }
          >
            <option value="dash">dash (-)</option>
            <option value="plus">plus (+)</option>
            <option value="star">star (*)</option>
          </select>
        </label>
        <label>
          Link URL style
          <select
            value={options.linkUrlStyle ?? "none"}
            onChange={(event) =>
              onChange({
                ...options,
                linkUrlStyle: event.target.value as Options["linkUrlStyle"],
              })
            }
          >
            <option value="none">none</option>
            <option value="angle">angle (&lt;url&gt;)</option>
          </select>
        </label>
        <label>
          Link title style
          <select
            value={options.linkTitleStyle ?? "paren"}
            onChange={(event) =>
              onChange({
                ...options,
                linkTitleStyle: event.target
                  .value as Options["linkTitleStyle"],
              })
            }
          >
            <option value="paren">paren ("title")</option>
            <option value="double">double ("title")</option>
            <option value="single">single ('title')</option>
          </select>
        </label>
      </div>
    </details>
  );
}
