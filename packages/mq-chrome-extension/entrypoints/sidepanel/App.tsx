import { useEffect, useRef, useState } from "react";
import { htmlToMarkdown, run, toHtml } from "./lib/mq";
import { extractActivePageHtml, toggleActivePagePreview } from "./lib/activeTab";
import { buildSrcDoc } from "./lib/buildSrcDoc";
import { SourcePane, type SourceMode } from "./components/SourcePane";
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
  const [html, setHtml] = useState("");
  const [sourceMode, setSourceMode] = useState<SourceMode>("markdown");
  const [query, setQuery] = useState("");
  const [result, setResult] = useState("");
  const [options, setOptions] = useState<RunOptions>(DEFAULT_OPTIONS);
  const [isExtracting, setIsExtracting] = useState(false);
  const [isRunning, setIsRunning] = useState(false);
  const [previewActive, setPreviewActive] = useState(false);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const copiedTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const source = sourceMode === "html" ? html : markdown;

  const handleExtract = async () => {
    setError(null);
    setIsExtracting(true);
    try {
      const extracted = await extractActivePageHtml();
      if (!extracted.ok) {
        setError(extracted.message);
        return;
      }
      setHtml(extracted.value);
      const md = await htmlToMarkdown(extracted.value);
      setMarkdown(md);
      setResult("");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsExtracting(false);
    }
  };

  useEffect(() => {
    handleExtract();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleRun = async () => {
    setError(null);
    setIsRunning(true);
    try {
      const filtered = await run(query, source, {
        ...options,
        inputFormat: sourceMode === "html" ? "html" : "markdown",
      });
      setResult(filtered);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsRunning(false);
    }
  };

  const handleCopy = () => {
    navigator.clipboard
      .writeText(result)
      .then(() => {
        setCopied(true);
        if (copiedTimeoutRef.current) clearTimeout(copiedTimeoutRef.current);
        copiedTimeoutRef.current = setTimeout(() => setCopied(false), 1500);
      })
      .catch((err: unknown) => {
        setError(err instanceof Error ? err.message : String(err));
      });
  };

  useEffect(() => {
    return () => {
      if (copiedTimeoutRef.current) clearTimeout(copiedTimeoutRef.current);
    };
  }, []);

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

      const previewSource =
        result || markdown || (html ? await htmlToMarkdown(html) : "");
      if (!previewSource) return;
      const previewHtml = await toHtml(previewSource);
      const srcdoc = buildSrcDoc(previewHtml);
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
      <div className="app-body">
        {error && <div className="error-banner">{error}</div>}

        <SourcePane
          mode={sourceMode}
          markdown={markdown}
          html={html}
          isExtracting={isExtracting}
          onModeChange={setSourceMode}
          onMarkdownChange={setMarkdown}
          onHtmlChange={setHtml}
          onExtract={handleExtract}
        />

        <QueryEditor
          query={query}
          isRunning={isRunning}
          disabled={!source}
          onChange={setQuery}
          onRun={handleRun}
        />

        <OptionsPanel options={options} onChange={setOptions} />

        <ResultPane
          result={result}
          previewActive={previewActive}
          previewDisabled={!result && !markdown && !html}
          copied={copied}
          onCopy={handleCopy}
          onTogglePreview={handleTogglePreview}
        />
      </div>
    </div>
  );
}
