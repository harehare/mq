import { CodeEditor } from "./CodeEditor";

type SourcePaneProps = {
  markdown: string;
  onMarkdownChange: (value: string) => void;
};

export function SourcePane({
  markdown,
  onMarkdownChange,
}: SourcePaneProps) {
  return (
    <section className="pane pane-source">
      <div className="pane-header">
        <h2>Source</h2>
      </div>
      <CodeEditor
        className="pane-textarea"
        language="markdown"
        placeholder="The current page is converted to Markdown automatically. You can also paste your own content."
        value={markdown}
        onChange={onMarkdownChange}
      />
    </section>
  );
}
