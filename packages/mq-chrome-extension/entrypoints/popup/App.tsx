import { useEffect, useRef, useState } from "react";
import { htmlToMarkdown, run } from "./lib/mq";
import { extractActivePageHtml } from "./lib/activeTab";
import { queryStorage } from "./lib/queryStorage";
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
  const [query, setQuery] = useState("");
  const [result, setResult] = useState("");
  const [options, setOptions] = useState<RunOptions>(DEFAULT_OPTIONS);
  const [isExtracting, setIsExtracting] = useState(false);
  const [isRunning, setIsRunning] = useState(false);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const copiedTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const queryLoadedRef = useRef(false);

  const handleExtract = async (): Promise<string | undefined> => {
    setError(null);
    setIsExtracting(true);
    try {
      const extracted = await extractActivePageHtml();
      if (!extracted.ok) {
        setError(extracted.message);
        return undefined;
      }
      const md = await htmlToMarkdown(extracted.value);
      setMarkdown(md);
      setResult("");
      return md;
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      return undefined;
    } finally {
      setIsExtracting(false);
    }
  };

  const runQuery = async (q: string, src: string, opts: RunOptions) => {
    setError(null);
    setIsRunning(true);
    try {
      const filtered = await run(q, src, opts);
      setResult(filtered);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsRunning(false);
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
      const [extractedMarkdown, storedQuery] = await Promise.all([
        handleExtract(),
        queryStorage.getValue(),
      ]);
      if (cancelled) return;
      setQuery(storedQuery);
      queryLoadedRef.current = true;
      if (storedQuery.trim() && extractedMarkdown) {
        await runQuery(storedQuery, extractedMarkdown, options);
      }
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

  return (
    <div className="app">
      <div className="app-body">
        {error && <div className="error-banner">{error}</div>}

        <SourcePane
          markdown={markdown}
          isExtracting={isExtracting}
          onMarkdownChange={setMarkdown}
          onExtract={handleExtract}
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

        <ResultPane result={result} copied={copied} onCopy={handleCopy} />
      </div>
    </div>
  );
}
