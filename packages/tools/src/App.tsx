import { useState, useRef, useEffect, useCallback } from "react";
import MarkdownIt from "markdown-it";
import Editor from "@monaco-editor/react";
import * as monaco from "monaco-editor";
import "./App.css";
import type { ViewMode } from "./types";
import { generateTreeView } from "./utils";
import { useResizer } from "./hooks/useResizer";
import { useDarkMode } from "./hooks/useDarkMode";
import { useLocalStorage } from "./hooks/useLocalStorage";
import { useCustomTools } from "./hooks/useCustomTools";
import { TREE_VIEW_SETTINGS } from "./constants";
import { tools } from "./tools";
import { CustomToolManager } from "./components/CustomToolManager";

const mdParser = new MarkdownIt();

function App() {
  const containerRef = useRef<HTMLDivElement>(
    null
  ) as React.RefObject<HTMLDivElement>;
  const editorRef = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);

  const [inputText, setInputText] = useLocalStorage("inputText", "");
  const [outputText, setOutputText] = useState("");
  const [viewMode, setViewMode] = useLocalStorage<ViewMode>("viewMode", "text");
  const [isTreeViewOpen, setIsTreeViewOpen] = useLocalStorage(
    "isTreeViewOpen",
    false
  );
  const [isOutputPanelOpen, setIsOutputPanelOpen] = useLocalStorage(
    "isOutputPanelOpen",
    true
  );
  const [isTransforming, setIsTransforming] = useState(false);
  const [isDragOver, setIsDragOver] = useState(false);
  const [selectedToolId, setSelectedToolId] = useLocalStorage(
    "selectedToolId",
    tools[0].id
  );
  const [showCustomToolManager, setShowCustomToolManager] = useState(false);

  const { isDarkMode, toggleDarkMode } = useDarkMode();
  const { leftPanelWidth, handleMouseDown } = useResizer({ containerRef });
  const { toolsFromCustom, refreshCustomTools } = useCustomTools();

  // Combine predefined and custom tools
  const allTools = [...tools, ...toolsFromCustom];

  // Find the current tool based on saved tool ID
  const selectedTool =
    allTools.find((tool) => tool.id === selectedToolId) || allTools[0];

  const handleToolChange = (newToolId: string) => {
    setSelectedToolId(newToolId);
  };

  const toggleTreeView = () => {
    setIsTreeViewOpen(!isTreeViewOpen);
  };

  const toggleOutputPanel = () => {
    setIsOutputPanelOpen(!isOutputPanelOpen);
  };

  const treeViewData = generateTreeView(inputText);

  const handleLineClick = (lineNumber: number) => {
    if (editorRef.current) {
      editorRef.current.revealLineInCenter(lineNumber);
      editorRef.current.setPosition({ lineNumber, column: 1 });
      editorRef.current.focus();
    }
  };

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(true);
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
  };

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);

    const files = Array.from(e.dataTransfer.files);
    const textFiles = files.filter(
      (file) =>
        file.type === "text/markdown" ||
        file.type === "text/plain" ||
        file.name.endsWith(".md") ||
        file.name.endsWith(".txt") ||
        file.name.endsWith(".markdown")
    );

    if (textFiles.length > 0) {
      const file = textFiles[0];
      try {
        const text = await file.text();
        setInputText(text);
      } catch (error) {
        console.error("Failed to read file:", error);
      }
    }
  };

  const handleCopyOutput = async () => {
    try {
      await navigator.clipboard.writeText(outputText);
    } catch (error) {
      console.error("Failed to copy output:", error);
    }
  };

  const handleTransform = useCallback(
    async (text: string) => {
      if (!text.trim()) {
        setOutputText("");
        return;
      }

      setIsTransforming(true);

      try {
        const result = await selectedTool.transform(text);
        setOutputText(result);
      } catch (error) {
        console.error("Transformation failed:", error);
        const errorMessage =
          error instanceof Error ? error.message : String(error);
        setOutputText(`Error: ${errorMessage}`);
      } finally {
        setIsTransforming(false);
      }
    },
    [selectedTool]
  );

  // Debounce effect for auto-transformation
  useEffect(() => {
    const timer = setTimeout(() => {
      handleTransform(inputText);
    }, 500); // 500ms delay

    return () => clearTimeout(timer);
  }, [inputText, handleTransform]);

  const handleCustomToolsChange = () => {
    refreshCustomTools();
  };

  const handleModalClose = () => {
    setShowCustomToolManager(false);
  };

  const handleOverlayClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget) {
      handleModalClose();
    }
  };

  // ESC key handler
  useEffect(() => {
    const handleEscapeKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && showCustomToolManager) {
        handleModalClose();
      }
    };

    if (showCustomToolManager) {
      document.addEventListener("keydown", handleEscapeKey);
      return () => {
        document.removeEventListener("keydown", handleEscapeKey);
      };
    }
  }, [showCustomToolManager]);

  return (
    <div className={`App ${isDarkMode ? "dark-mode" : ""}`}>
      {showCustomToolManager && (
        <div className="modal-overlay" onClick={handleOverlayClick}>
          <CustomToolManager
            onClose={handleModalClose}
            onToolsChanged={handleCustomToolsChange}
          />
        </div>
      )}
      <header className="app-header">
        <div className="header-left">
          <a
            href="https://github.com/harehare/mq"
            target="_blank"
            rel="noopener noreferrer"
            className="mq-link"
            title="Powered by mq"
          >
            <img src="/logo.svg" alt="mq" style={{ width: "32px" }} />
          </a>
          <h1>Markdown Tools</h1>
          <p className="header-subtitle">
            A collection of useful tools for working with Markdown documents.
            Transform, analyze, and process your Markdown content with ease.
          </p>
          <div className="privacy-indicator">🔒 Local 100% • Telemetry 0%</div>
        </div>
        <div className="header-controls">
          <button
            onClick={() => setShowCustomToolManager(true)}
            className="manage-tools-toggle"
            title="Manage custom tools"
          >
            ⚙️
          </button>
          <a
            href="https://mqlang.org"
            target="_blank"
            rel="noopener noreferrer"
            className="mq-logo-link"
            title="mq - Command-line tool for processing Markdown with jq-like syntax"
          >
            <img src="/logo.svg" alt="mq" className="mq-logo-small" />
          </a>
          <button onClick={toggleTreeView} className="tree-view-toggle">
            {isTreeViewOpen ? "📖" : "🌲"}
          </button>
          <button onClick={toggleOutputPanel} className="output-panel-toggle">
            {isOutputPanelOpen ? "📄" : "📄"}
          </button>
          <button onClick={toggleDarkMode} className="theme-toggle">
            {isDarkMode ? "☀️" : "🌙"}
          </button>
          <a
            href="https://github.com/harehare/mq"
            target="_blank"
            rel="noopener noreferrer"
          >
            <img src="https://img.shields.io/github/stars/harehare/mq?style=social" />
          </a>
        </div>
      </header>

      <div className="toolbar">
        <select
          onChange={(e) => handleToolChange(e.target.value)}
          value={selectedTool.id}
        >
          {Array.from(new Set(allTools.map((tool) => tool.category)))
            .sort()
            .map((category) => (
              <optgroup key={category} label={category}>
                {allTools
                  .filter((tool) => tool.category === category)
                  .map((tool) => (
                    <option key={tool.id} value={tool.id}>
                      {tool.name}
                    </option>
                  ))}
              </optgroup>
            ))}
        </select>
        <span className="tool-description">{selectedTool.description}</span>
      </div>

      <div className="main-layout">
        {isTreeViewOpen && (
          <div className="tree-view">
            <div className="tree-view-header">
              <h3>Document Outline</h3>
            </div>
            <div className="tree-view-content">
              {treeViewData.length > 0 ? (
                <ul className="tree-list">
                  {treeViewData.map((heading, index) => (
                    <li
                      key={index}
                      className={`tree-item level-${heading.level}`}
                      style={{
                        marginLeft: `${
                          (heading.level - 1) *
                          TREE_VIEW_SETTINGS.INDENT_PX_PER_LEVEL
                        }px`,
                      }}
                      onClick={() => handleLineClick(heading.line)}
                    >
                      <span className="heading-text">{heading.text}</span>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="empty-tree">No headings found</p>
              )}
            </div>
          </div>
        )}
        <div className="container" ref={containerRef}>
          <div
            className={`input-area ${isDragOver ? "drag-over" : ""}`}
            style={{ width: isOutputPanelOpen ? `${leftPanelWidth}%` : "100%" }}
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
            onDrop={handleDrop}
          >
            <div className="editor-header">
              <h2>Input</h2>
              <div className="drag-hint">
                {isDragOver ? "Drop file here" : "Drag & drop markdown files"}
              </div>
            </div>
            <div className="editor-content">
              <Editor
                height="100%"
                defaultLanguage="markdown"
                value={inputText}
                onChange={(value) => setInputText(value || "")}
                onMount={(editor) => {
                  editorRef.current = editor;
                }}
                options={{
                  minimap: { enabled: false },
                  scrollBeyondLastLine: false,
                  fontSize: 14,
                  wordWrap: "off",
                  automaticLayout: true,
                  fontFamily:
                    "'JetBrains Mono', 'Source Code Pro', Menlo, monospace",
                  lineHeight: 1.6,
                  theme: isDarkMode ? "vs-dark" : "vs",
                }}
                theme={isDarkMode ? "vs-dark" : "vs"}
              />
            </div>
          </div>
          {isOutputPanelOpen && (
            <>
              <div className="resizer" onMouseDown={handleMouseDown}></div>
              <div
                className="output-area"
                style={{ width: `${100 - leftPanelWidth}%` }}
              >
                <div className="editor-header">
                  <h2>Output</h2>
                  <div className="output-controls">
                    <button
                      onClick={handleCopyOutput}
                      className="copy-button"
                      disabled={!outputText.trim()}
                      title="Copy output to clipboard"
                    >
                      📋 Copy
                    </button>
                    <div className="view-mode-toggle">
                      <button
                        onClick={() => setViewMode("text")}
                        className={viewMode === "text" ? "active" : ""}
                      >
                        Text
                      </button>
                      <button
                        onClick={() => setViewMode("preview")}
                        className={viewMode === "preview" ? "active" : ""}
                      >
                        Preview
                      </button>
                    </div>
                  </div>
                </div>
                <div className="editor-content">
                  {viewMode === "text" ? (
                    <Editor
                      height="100%"
                      defaultLanguage="markdown"
                      value={isTransforming ? "Processing..." : outputText}
                      options={{
                        readOnly: true,
                        minimap: { enabled: false },
                        scrollBeyondLastLine: false,
                        fontSize: 14,
                        automaticLayout: true,
                        fontFamily:
                          "'JetBrains Mono', 'Source Code Pro', Menlo, monospace",
                        lineHeight: 1.6,
                        wordWrap: "off",
                      }}
                      theme={isDarkMode ? "vs-dark" : "vs"}
                    />
                  ) : (
                    <div
                      className="preview"
                      dangerouslySetInnerHTML={{
                        __html: isTransforming
                          ? "<p>Processing...</p>"
                          : mdParser.render(outputText),
                      }}
                    />
                  )}
                </div>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

export default App;
