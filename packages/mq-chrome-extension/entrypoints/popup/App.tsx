import { useEffect, useRef, useState } from "react";
import { diagnostics, htmlToMarkdown, run } from "./lib/mq";
import { extractActivePageHtml } from "./lib/activeTab";
import { DEFAULT_QUERY, queryStorage } from "./lib/queryStorage";
import { downloadText } from "./lib/download";
import { formatDiagnostics } from "./lib/diagnostics";
import { SourcePane } from "./components/SourcePane";
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
  const [query, setQuery] = useState(DEFAULT_QUERY);
  const [result, setResult] = useState("");
  const [options, setOptions] = useState<RunOptions>(DEFAULT_OPTIONS);
  const [isRunning, setIsRunning] = useState(false);
  const [copied, setCopied] = useState(false);
  const [lastRun, setLastRun] = useState<{ durationMs: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const copiedTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const autoRunTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const queryLoadedRef = useRef(false);
  const runIdRef = useRef(0);

  const handleExtract = async (): Promise<string | undefined> => {
    setError(null);
    try {
      const extracted = await extractActivePageHtml();
      if (!extracted.ok) {
        setError(extracted.message);
        return undefined;
      }
      const md = await htmlToMarkdown(extracted.value);
      setMarkdown(md);
      setResult("");
      setLastRun(null);
      return md;
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      return undefined;
    }
  };

  const runQuery = async (q: string, src: string, opts: RunOptions) => {
    const runId = ++runIdRef.current;
    if (autoRunTimeoutRef.current) {
      clearTimeout(autoRunTimeoutRef.current);
      autoRunTimeoutRef.current = null;
    }
    setError(null);
    setIsRunning(true);
    const startedAt = performance.now();
    try {
      const filtered = await run(q, src, opts);
      if (runId !== runIdRef.current) return;
      setResult(filtered);
      setLastRun({ durationMs: Math.round(performance.now() - startedAt) });
    } catch (err) {
      const fallback = err instanceof Error ? err.message : String(err);
      try {
        const queryDiagnostics = await diagnostics(q);
        if (runId !== runIdRef.current) return;
        setError(
          queryDiagnostics.length > 0
            ? formatDiagnostics(queryDiagnostics)
            : fallback,
        );
      } catch {
        if (runId !== runIdRef.current) return;
        setError(fallback);
      }
    } finally {
      if (runId === runIdRef.current) setIsRunning(false);
    }
  };

  const handleRun = () => runQuery(query, markdown, options);

  const handleSelectExample = (code: string) => {
    setQuery(code);
    if (markdown) runQuery(code, markdown, options);
  };

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const [, storedQuery] = await Promise.all([
        handleExtract(),
        queryStorage.getValue(),
      ]);
      if (cancelled) return;
      setQuery(storedQuery);
      queryLoadedRef.current = true;
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!queryLoadedRef.current) return;
    queryStorage.setValue(query);
  }, [query]);

  useEffect(() => {
    if (!queryLoadedRef.current || !query.trim() || !markdown) return;
    autoRunTimeoutRef.current = setTimeout(() => {
      void runQuery(query, markdown, options);
    }, 400);
    return () => {
      if (autoRunTimeoutRef.current) {
        clearTimeout(autoRunTimeoutRef.current);
        autoRunTimeoutRef.current = null;
      }
    };
  }, [markdown, options, query]);

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

  const handleDownload = () => {
    downloadText("mq-result.md", result);
  };

  useEffect(() => {
    return () => {
      if (copiedTimeoutRef.current) clearTimeout(copiedTimeoutRef.current);
      if (autoRunTimeoutRef.current) clearTimeout(autoRunTimeoutRef.current);
    };
  }, []);

  return (
    <div className="app">
      <div className="app-body">
        {error && <div className="error-banner">{error}</div>}

        <SourcePane
          markdown={markdown}
          onMarkdownChange={setMarkdown}
        />

        <QueryEditor
          query={query}
          isRunning={isRunning}
          disabled={!markdown}
          onChange={setQuery}
          onSelectExample={handleSelectExample}
          onRun={handleRun}
        />

        <OptionsPanel options={options} onChange={setOptions} />

        <ResultPane
          result={result}
          copied={copied}
          lastRun={lastRun}
          onCopy={handleCopy}
          onDownload={handleDownload}
        />
      </div>
    </div>
  );
}
