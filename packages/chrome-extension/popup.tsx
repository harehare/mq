import React, { useState, useEffect } from 'react';
import { createRoot } from 'react-dom/client';

const Popup: React.FC = () => {
  const [query, setQuery] = useState<string>('');
  const [result, setResult] = useState<string>('');
  const [error, setError] = useState<string>('');

  useEffect(() => {
    // Listen for messages from the background script
    chrome.runtime.onMessage.addListener((message) => {
      if (message.type === 'MQ_RESULT') {
        if (message.error) {
          setError(message.error);
          setResult('');
        } else {
          setResult(message.data || '');
          setError('');
        }
      }
    });
  }, []);

  const handleRunQuery = () => {
    setError('');
    setResult('Running...');
    chrome.runtime.sendMessage({ type: 'RUN_MQ_QUERY', query });
  };

  const handleCopyToClipboard = () => {
    navigator.clipboard.writeText(result).then(() => {
      // Maybe show a small notification "Copied!"
    }).catch(err => {
      setError('Failed to copy to clipboard: ' + err);
    });
  };

  return (
    <div className="container">
      <div className="row">
        <textarea
          placeholder="Enter your mq query here..."
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>
      <div className="actions">
        <button onClick={handleRunQuery}>Run Query</button>
      </div>
      {error && <div style={{ color: 'red', marginTop: '10px' }}>Error: {error}</div>}
      <div className="row" style={{ marginTop: '10px' }}>
        <textarea
          placeholder="Results will appear here..."
          value={result}
          readOnly
        />
      </div>
      {result && !error && (
        <div className="actions" style={{ marginTop: '-5px' }}>
          <button onClick={handleCopyToClipboard}>Copy Result</button>
        </div>
      )}
    </div>
  );
};

const container = document.getElementById('root');
if (container) {
  const root = createRoot(container);
  root.render(<Popup />);
}
