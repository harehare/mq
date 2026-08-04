import { useState } from "react";
import { htmlToMarkdown, run, toHtml } from "./lib/mq";
import { extractActivePageHtml, toggleActivePagePreview } from "./lib/activeTab";
import { buildSrcDoc } from "./lib/buildSrcDoc";
import { MarkdownPane } from "./components/MarkdownPane";
import { QueryEditor } from "./components/QueryEditor";
import { ResultPane } from "./components/ResultPane";
import { OptionsPanel, type RunOptions } from "./components/OptionsPanel";

const DEFAULT_OPTIONS: RunOptions = {
  listStyle: "dash",
  linkUrlStyle: "none",
  linkTitleStyle: "paren",
};

export function App() {
  const [markdown, setMarkdown] = useState("");
  const [query, setQuery] = useState("");
  const [result, setResult] = useState("");
  const [options, setOptions] = useState<RunOptions>(DEFAULT_OPTIONS);
  const [isExtracting, setIsExtracting] = useState(false);
  const [isRunning, setIsRunning] = useState(false);
  const [previewActive, setPreviewActive] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleExtract = async () => {
    setError(null);
    setIsExtracting(true);
    try {
      const extracted = await extractActivePageHtml();
      if (!extracted.ok) {
        setError(extracted.message);
        return;
      }
      const md = await htmlToMarkdown(extracted.value);
      setMarkdown(md);
      setResult("");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsExtracting(false);
    }
  };

  const handleRun = async () => {
    setError(null);
    setIsRunning(true);
    try {
      const filtered = await run(query, markdown, options);
      setResult(filtered);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsRunning(false);
    }
  };

  const handleCopy = () => {
    navigator.clipboard.writeText(result).catch((err: unknown) => {
      setError(err instanceof Error ? err.message : String(err));
    });
  };

  const handleTogglePreview = async () => {
    setError(null);
    try {
      if (previewActive) {
        const toggled = await toggleActivePagePreview("");
        if (!toggled.ok) {
          setError(toggled.message);
          return;
        }
        setPreviewActive(toggled.value);
        return;
      }

      const source = result || markdown;
      if (!source) return;
      const html = await toHtml(source);
      const srcdoc = buildSrcDoc(html);
      const toggled = await toggleActivePagePreview(srcdoc);
      if (!toggled.ok) {
        setError(toggled.message);
        return;
      }
      setPreviewActive(toggled.value);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <div className="app">
      <header className="app-header">
        <h1>mq</h1>
        <p>Convert this page to Markdown, filter it, and preview the result.</p>
      </header>

      {error && <div className="error-banner">{error}</div>}

      <MarkdownPane
        markdown={markdown}
        isExtracting={isExtracting}
        onChange={setMarkdown}
        onExtract={handleExtract}
      />

      <QueryEditor
        query={query}
        isRunning={isRunning}
        disabled={!markdown}
        onChange={setQuery}
        onRun={handleRun}
      />

      <OptionsPanel options={options} onChange={setOptions} />

      <ResultPane
        result={result}
        previewActive={previewActive}
        previewDisabled={!result && !markdown}
        onCopy={handleCopy}
        onTogglePreview={handleTogglePreview}
      />
    </div>
  );
}
