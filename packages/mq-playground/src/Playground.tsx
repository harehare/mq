import { useState, useEffect, useCallback, useRef } from "react";
import Editor, { Monaco } from "@monaco-editor/react";
import "./index.css";
import "./vim.css";
import * as mq from "mq-web";
import { toHtml } from "mq-web";
import { languages, editor, IPosition } from "monaco-editor";
import LZString from "lz-string";
import { FileTree } from "./components/FileTree";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { SettingsDialog } from "./components/SettingsDialog";
import { TabBar, Tab } from "./components/TabBar";
import { ResizeHandle } from "./components/ResizeHandle";
import { ToastContainer, ToastItem } from "./components/Toast";
import { ExamplesModal } from "./components/ExamplesModal";
import { fileSystem, FileNode, OPFSFileSystem } from "./utils/fileSystem";
import { isDesktop, isMobile } from "./utils/deviceDetection";
import { ProblemsPanel, DiagnosticItem } from "./components/ProblemsPanel";
import {
  VscLayoutSidebarLeft,
  VscLayoutSidebarLeftOff,
  VscSave,
  VscCheck,
  VscLoading,
  VscWordWrap,
  VscMap,
  VscSettingsGear,
  VscPlay,
  VscSymbolMethod,
  VscLinkExternal,
  VscCopy,
  VscListUnordered,
  VscError,
  VscWarning,
} from "react-icons/vsc";
import { initVimMode, VimMode } from "monaco-vim";
import { EXAMPLES } from "./examples";

type SharedData = {
  code: string;
  markdown: string;
  options: mq.Options;
};

type AllFilesSharedData = {
  type: "all-files";
  files: Array<{ path: string; content: string }>;
  options: mq.Options;
  activeFile?: string;
};

const getAllFilesFlat = (nodes: FileNode[]): FileNode[] => {
  const result: FileNode[] = [];
  for (const node of nodes) {
    if (node.type === "file") {
      result.push(node);
    } else if (node.children) {
      result.push(...getAllFilesFlat(node.children));
    }
  }
  return result;
};

const CODE_KEY = "mq-playground.code";
const MARKDOWN_KEY = "mq-playground.markdown";
const IS_UPDATE_KEY = "mq-playground.is_update";
const ENABLE_TYPE_CHECK_KEY = "mq-playground.enable_type_check";
const INPUT_FORMAT_KEY = "mq-playground.input_format";
const SELECTED_FILE_KEY = "mq-playground.selected_file";
const CURRENT_FILE_PATH_KEY = "mq-playground.current_file_path";
const SIDEBAR_VISIBLE_KEY = "mq-playground.sidebar-visible";
const TABS_KEY = "mq-playground.tabs";
const ACTIVE_TAB_ID_KEY = "mq-playground.active_tab_id";
const SIDEBAR_WIDTH_KEY = "mq-playground.sidebar-width";
const LEFT_RIGHT_SPLIT_KEY = "mq-playground.left-right-split";
const TOP_BOTTOM_SPLIT_KEY = "mq-playground.top-bottom-split";
const EDITOR_SETTINGS_KEY = "mq-playground.editor-settings";
const PROBLEMS_PANEL_HEIGHT_KEY = "mq-playground.problems-panel-height";

type EditorSettings = {
  version: number;
  minimapEnabled: boolean;
  wordWrap: "on" | "off";
  vimModeEnabled: boolean;
  fontSize: number;
  theme: "light" | "dark" | "system" | "mq";
  lineNumbers: "on" | "off";
  tabSize: number;
  shareMode: "current" | "all";
};

const DEFAULT_EDITOR_SETTINGS: EditorSettings = {
  version: 2,
  minimapEnabled: false,
  wordWrap: "off",
  vimModeEnabled: false,
  fontSize: 12,
  theme: "mq",
  lineNumbers: "on",
  tabSize: 2,
  shareMode: "current",
};

function loadEditorSettings(): EditorSettings {
  try {
    const raw = localStorage.getItem(EDITOR_SETTINGS_KEY);
    if (!raw) return DEFAULT_EDITOR_SETTINGS;
    const stored = JSON.parse(raw) as Partial<EditorSettings>;
    // Migrate from version 1: reset theme to mq default
    if (!stored.version || stored.version < 2) {
      return {
        ...DEFAULT_EDITOR_SETTINGS,
        ...stored,
        theme: "mq",
        version: DEFAULT_EDITOR_SETTINGS.version,
      };
    }
    return {
      ...DEFAULT_EDITOR_SETTINGS,
      ...stored,
      version: DEFAULT_EDITOR_SETTINGS.version,
    };
  } catch {
    return DEFAULT_EDITOR_SETTINGS;
  }
}

function saveEditorSettings(settings: Partial<EditorSettings>) {
  try {
    const current = loadEditorSettings();
    localStorage.setItem(
      EDITOR_SETTINGS_KEY,
      JSON.stringify({
        ...current,
        ...settings,
        version: DEFAULT_EDITOR_SETTINGS.version,
      }),
    );
  } catch {
    // ignore
  }
}

export const Playground = () => {
  const [code, setCode] = useState<string | undefined>(
    localStorage.getItem(CODE_KEY) ?? EXAMPLES[0].code,
  );
  const [markdown, setMarkdown] = useState<string | undefined>(
    localStorage.getItem(MARKDOWN_KEY) ?? EXAMPLES[0].markdown,
  );
  const [isUpdate, setIsUpdate] = useState(
    localStorage.getItem(IS_UPDATE_KEY) === "true",
  );
  const [enableTypeCheck, setEnableTypeCheck] = useState(
    localStorage.getItem(ENABLE_TYPE_CHECK_KEY) === "true",
  );
  const [executionTime, setExecutionTime] = useState<number | null>(null);
  const [isEmbed, setIsEmbed] = useState(false);
  const [result, setResult] = useState("");
  const [listStyle, setListStyle] = useState<mq.Options["listStyle"]>(null);
  const [linkUrlStyle, setLinkUrlStyle] =
    useState<mq.Options["linkUrlStyle"]>(null);
  const [linkTitleStyle, setLinkTitleStyle] =
    useState<mq.Options["linkTitleStyle"]>(null);
  const [inputFormat, setInputFormat] = useState<mq.Options["inputFormat"]>(
    (() => {
      const format = localStorage.getItem(INPUT_FORMAT_KEY);
      return format === "markdown"
        ? "markdown"
        : format === "text"
          ? "text"
          : format === "mdx"
            ? "mdx"
            : format === "html"
              ? "html"
              : format === "raw"
                ? "raw"
                : format === "null"
                  ? "null"
                  : null;
    })(),
  );
  const [activeTab, setActiveTab] = useState<"output" | "ast" | "preview">(
    "output",
  );
  const [astResult, setAstResult] = useState("");
  const [previewHtml, setPreviewHtml] = useState("");
  const [files, setFiles] = useState<FileNode[]>([]);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [currentFilePath, setCurrentFilePath] = useState<string | null>(null);
  const [isOPFSSupported, setIsOPFSSupported] = useState<boolean>(false);
  const [isSidebarVisible, setIsSidebarVisible] = useState<boolean>(() => {
    // Check URL parameter first
    const urlParams = new URLSearchParams(window.location.search);
    const sidebarParam = urlParams.get("sidebar");

    if (sidebarParam !== null) {
      return sidebarParam === "true";
    }

    // On mobile, default to hidden
    if (isMobile()) {
      return false;
    }

    // Desktop: check localStorage, default to true if not set
    const storedValue = localStorage.getItem(SIDEBAR_VISIBLE_KEY);
    return storedValue === null ? true : storedValue !== "false";
  });
  const [saveStatus, setSaveStatus] = useState<"saved" | "saving" | "unsaved">(
    "saved",
  );
  const [deleteConfirmDialog, setDeleteConfirmDialog] = useState<{
    path: string;
  } | null>(null);
  const [isRenaming, setIsRenaming] = useState(false);
  const [sidebarWidth, setSidebarWidth] = useState(() => {
    const stored = Number(localStorage.getItem(SIDEBAR_WIDTH_KEY));
    return stored > 0 ? stored : 250;
  });
  const [leftRightSplit, setLeftRightSplit] = useState(() => {
    const stored = Number(localStorage.getItem(LEFT_RIGHT_SPLIT_KEY));
    return stored > 0 ? stored : 50;
  });
  const [topBottomSplit, setTopBottomSplit] = useState(() => {
    const stored = Number(localStorage.getItem(TOP_BOTTOM_SPLIT_KEY));
    return stored > 0 ? stored : 50;
  });
  const _initialSettings = loadEditorSettings();
  const [minimapEnabled, setMinimapEnabled] = useState(
    _initialSettings.minimapEnabled,
  );
  const [wordWrap, setWordWrap] = useState<"on" | "off">(
    _initialSettings.wordWrap,
  );
  const [lineNumbers, setLineNumbers] = useState<"on" | "off">(
    _initialSettings.lineNumbers,
  );
  const [tabSize, setTabSize] = useState(_initialSettings.tabSize);
  const [vimModeEnabled, setVimModeEnabled] = useState(
    _initialSettings.vimModeEnabled,
  );
  const [fontSize, setFontSize] = useState(_initialSettings.fontSize);
  const [theme, setTheme] = useState<"light" | "dark" | "system" | "mq">(
    _initialSettings.theme,
  );
  const [shareMode, setShareMode] = useState<"current" | "all">(
    _initialSettings.shareMode,
  );
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isExamplesOpen, setIsExamplesOpen] = useState(false);
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const [diagnostics, setDiagnostics] = useState<{
    errors: number;
    warnings: number;
    items: DiagnosticItem[];
  }>({ errors: 0, warnings: 0, items: [] });
  const [isProblemsVisible, setIsProblemsVisible] = useState(false);
  const [problemsPanelHeight, setProblemsPanelHeight] = useState(() => {
    const stored = Number(localStorage.getItem(PROBLEMS_PANEL_HEIGHT_KEY));
    return stored > 0 ? stored : 160;
  });
  const [tabCloseConfirm, setTabCloseConfirm] = useState<{
    tabId: string;
    filePath: string;
  } | null>(null);
  const [cursorPosition, setCursorPosition] = useState({
    line: 1,
    column: 1,
  });
  const contentRef = useRef<HTMLDivElement>(null);
  const leftPanelRef = useRef<HTMLDivElement>(null);
  const [tabs, setTabs] = useState<Tab[]>(() => {
    // Restore tabs from localStorage on initial load
    try {
      const savedTabs = localStorage.getItem(TABS_KEY);
      return savedTabs ? JSON.parse(savedTabs) : [];
    } catch {
      return [];
    }
  });
  const [activeTabId, setActiveTabId] = useState<string | null>(() => {
    // Restore active tab ID from localStorage on initial load
    return localStorage.getItem(ACTIVE_TAB_ID_KEY);
  });
  const hasInitialized = useRef(false);
  const enableTypeCheckRef = useRef(enableTypeCheck);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const vimModeRef = useRef<any>(null);
  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);
  const vimStatusBarRef = useRef<HTMLDivElement>(null);
  // Refs to allow vim Ex commands to access the latest callbacks without re-registering
  const saveCurrentFileRef = useRef<() => Promise<void>>(async () => {});
  const activeTabIdRef = useRef<string | null>(null);
  const handleTabCloseRef = useRef<(tabId: string) => void>(() => {});
  const diagnosticsTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );

  const showToast = useCallback(
    (message: string, type: ToastItem["type"] = "success") => {
      const id = `toast-${Date.now()}`;
      setToasts((prev) => [...prev, { id, message, type }]);
    },
    [],
  );

  // Keep enableTypeCheckRef in sync with state
  useEffect(() => {
    enableTypeCheckRef.current = enableTypeCheck;
  }, [enableTypeCheck]);

  // Persist tabs to localStorage whenever they change
  useEffect(() => {
    if (tabs.length > 0) {
      localStorage.setItem(TABS_KEY, JSON.stringify(tabs));
    } else {
      localStorage.removeItem(TABS_KEY);
    }
  }, [tabs]);

  // Persist active tab ID to localStorage whenever it changes
  useEffect(() => {
    if (activeTabId) {
      localStorage.setItem(ACTIVE_TAB_ID_KEY, activeTabId);
    } else {
      localStorage.removeItem(ACTIVE_TAB_ID_KEY);
    }
  }, [activeTabId]);

  const loadFiles = useCallback(async () => {
    try {
      const fileList = await fileSystem.listFiles("/");
      setFiles(fileList);
    } catch (error) {
      console.error("Failed to load files:", error);
    }
  }, []);

  useEffect(() => {
    // Prevent double initialization in React 18 Strict Mode
    if (hasInitialized.current) {
      return;
    }
    hasInitialized.current = true;

    const initFileSystem = async () => {
      // Check if OPFS is supported
      const supported = OPFSFileSystem.isSupported();
      setIsOPFSSupported(supported);

      if (!supported) {
        console.warn(
          "Origin Private File System is not supported in this browser",
        );
        return;
      }

      try {
        await fileSystem.initialize();
        await loadFiles();

        // Handle all-files share restoration from URL hash
        if (window.location.hash) {
          try {
            const compressed = window.location.hash.substring(1);
            const decompressed =
              LZString.decompressFromEncodedURIComponent(compressed);
            if (decompressed) {
              const parsedData = JSON.parse(decompressed);
              if (
                parsedData.type === "all-files" &&
                Array.isArray(parsedData.files)
              ) {
                for (const file of parsedData.files as {
                  path: string;
                  content: string;
                }[]) {
                  await fileSystem.writeFile(file.path, file.content);
                }
                await loadFiles();
                const options = parsedData.options || {};
                setIsUpdate(!!options.isUpdate);
                setInputFormat(options.inputFormat || null);
                setListStyle(options.listStyle || null);
                setLinkUrlStyle(options.linkUrlStyle || null);
                setLinkTitleStyle(options.linkTitleStyle || null);
                const activeFilePath =
                  parsedData.activeFile || parsedData.files[0]?.path;
                if (activeFilePath) {
                  const content = await fileSystem.readFile(activeFilePath);
                  openOrSwitchToTab(activeFilePath, content);
                  setSelectedFile(activeFilePath);
                }
                return;
              }
            }
          } catch (e) {
            console.error("Failed to restore all-files share:", e);
          }
        }

        // Get initial tabs and activeTabId from localStorage
        const savedTabs = localStorage.getItem(TABS_KEY);
        const initialTabs = savedTabs ? JSON.parse(savedTabs) : [];
        const savedActiveTabId = localStorage.getItem(ACTIVE_TAB_ID_KEY);

        // Restore active tab content if tabs were restored from localStorage
        if (
          !window.location.hash &&
          savedActiveTabId &&
          initialTabs.length > 0
        ) {
          const activeTab = initialTabs.find(
            (tab: Tab) => tab.id === savedActiveTabId,
          );
          if (activeTab) {
            try {
              // Verify the file still exists and load its current content
              const content = await fileSystem.readFile(activeTab.filePath);
              setEditorContent(activeTab.filePath, content);
              setSelectedFile(activeTab.filePath);
            } catch (error) {
              console.error("Failed to restore active tab:", error);
              // If active tab file doesn't exist, clear it
              setTabs((prev) =>
                prev.filter((tab) => tab.id !== savedActiveTabId),
              );
              setActiveTabId(null);
            }
          }
        } else if (!window.location.hash && initialTabs.length === 0) {
          // Only open a new tab if no tabs were restored
          const savedFilePath = localStorage.getItem(CURRENT_FILE_PATH_KEY);
          if (savedFilePath) {
            try {
              const content = await fileSystem.readFile(savedFilePath);
              openOrSwitchToTab(savedFilePath, content);
              setSelectedFile(savedFilePath);
            } catch (error) {
              console.error("Failed to restore last file:", error);
              localStorage.removeItem(CURRENT_FILE_PATH_KEY);
              localStorage.removeItem(SELECTED_FILE_KEY);
            }
          }
        }
      } catch (error) {
        console.error("Failed to initialize file system:", error);
        setIsOPFSSupported(false);
      }
    };

    initFileSystem();
    // eslint-disable-next-line react-hooks/exhaustive-deps

    if (window.location.hash) {
      try {
        const compressed = window.location.hash.substring(1);
        const decompressed =
          LZString.decompressFromEncodedURIComponent(compressed);
        if (decompressed) {
          const parsedData = JSON.parse(decompressed);
          // all-files shares are restored inside initFileSystem after OPFS init
          if (parsedData.type !== "all-files") {
            const options = parsedData.options || {};
            const data: SharedData = {
              code: typeof parsedData.code === "string" ? parsedData.code : "",
              markdown:
                typeof parsedData.markdown === "string"
                  ? parsedData.markdown
                  : "",
              options: {
                isUpdate: !!options.isUpdate,
                inputFormat: options.inputFormat || null,
                listStyle: options.listStyle,
                linkUrlStyle: options.linkUrlStyle || null,
                linkTitleStyle: options.linkTitleStyle || null,
              },
            };
            setCode(data.code);
            setMarkdown(data.markdown);
            setIsUpdate(data.options.isUpdate === true);
            setInputFormat(data.options.inputFormat);
            setListStyle(data.options.listStyle);
            setLinkUrlStyle(data.options.linkUrlStyle);
            setLinkTitleStyle(data.options.linkTitleStyle);
          }
        }
      } catch {
        showToast("Failed to load shared playground", "error");
      }
    }

    const urlParams = new URLSearchParams(window.location.search);
    const embedParam = urlParams.get("embed");
    setIsEmbed(embedParam === "true");

    const themeParam = urlParams.get("theme");
    if (themeParam) {
      const isDark = themeParam === "dark" || themeParam === "mq";
      document.documentElement.style.colorScheme = isDark ? "dark" : "light";
      if (themeParam === "mq") {
        document.documentElement.setAttribute("data-theme", "mq");
      } else {
        document.documentElement.removeAttribute("data-theme");
      }
      document.documentElement.style.setProperty(
        "--lightningcss-light",
        isDark ? " " : "initial",
      );
      document.documentElement.style.setProperty(
        "--lightningcss-dark",
        isDark ? "initial" : " ",
      );
    }

    // Update sidebar visibility from URL parameter if present
    const sidebarParam = urlParams.get("sidebar");
    if (sidebarParam !== null) {
      setIsSidebarVisible(sidebarParam === "true");
    }
  }, [loadFiles]);

  const [isFirstRun, setIsFirstRun] = useState(true);

  const dismissToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const handleRun = useCallback(async () => {
    setIsFirstRun(false);

    if (!code) {
      return;
    }
    setResult(isFirstRun ? "Initializing..." : "Running...");
    setAstResult("");
    setPreviewHtml("");
    setExecutionTime(null);

    const startTime = performance.now();

    try {
      const output = await mq.run(code, markdown ?? "", {
        isUpdate,
        inputFormat,
        listStyle,
        linkTitleStyle,
        linkUrlStyle,
      });
      setResult(output);

      if (activeTab === "preview") {
        setPreviewHtml(await toHtml(output));
      }
    } catch (e) {
      setResult((e as Error).toString());
      setPreviewHtml("");
    } finally {
      const endTime = performance.now();
      setExecutionTime(endTime - startTime);
    }
  }, [
    activeTab,
    code,
    markdown,
    inputFormat,
    isUpdate,
    listStyle,
    linkUrlStyle,
    linkTitleStyle,
  ]);

  const handleGenerateAst = useCallback(async () => {
    if (!code) {
      return;
    }
    setAstResult("Generating AST...");
    setExecutionTime(null);

    const startTime = performance.now();

    try {
      const ast = await mq.toAst(code);
      setAstResult(JSON.stringify(JSON.parse(ast), null, "  "));
    } catch (e) {
      setAstResult((e as Error).toString());
    } finally {
      const endTime = performance.now();
      setExecutionTime(endTime - startTime);
    }
  }, [code]);

  const handleFormat = useCallback(async () => {
    if (!code) {
      return;
    }

    setCode(await mq.format(code));
  }, [code]);

  const handleChangeExample = useCallback((index: number) => {
    const selected = EXAMPLES[index];
    setCode(selected.code);
    setMarkdown(selected.markdown);
    setIsUpdate(selected.isUpdate);
    setInputFormat(selected.format);
  }, []);

  const handleShare = useCallback(async () => {
    const options: mq.Options = {
      isUpdate: isUpdate || false,
      inputFormat: inputFormat || null,
      listStyle: listStyle || null,
      linkUrlStyle: linkUrlStyle || null,
      linkTitleStyle: linkTitleStyle || null,
    };

    let shareData: SharedData | AllFilesSharedData;

    if (shareMode === "all" && isOPFSSupported && files.length > 0) {
      try {
        const flatFiles = getAllFilesFlat(files);
        const filesData = await Promise.all(
          flatFiles.map(async (file) => ({
            path: file.path,
            content:
              file.path === currentFilePath && code !== undefined
                ? code
                : await fileSystem.readFile(file.path),
          })),
        );
        shareData = {
          type: "all-files",
          files: filesData,
          options,
          activeFile: currentFilePath || undefined,
        };
      } catch {
        showToast("Failed to share files", "error");
        return;
      }
    } else {
      shareData = {
        code: code || "",
        markdown: markdown || "",
        options,
      };
    }

    const compressed = LZString.compressToEncodedURIComponent(
      JSON.stringify(shareData),
    );
    const url = `${window.location.origin}${window.location.pathname}#${compressed}`;
    window.location.hash = compressed;

    navigator.clipboard
      .writeText(url)
      .then(() => {
        showToast("Share URL copied to clipboard!", "success");
      })
      .catch(() => {
        showToast("Failed to copy URL", "error");
      });
  }, [
    shareMode,
    isOPFSSupported,
    files,
    code,
    markdown,
    inputFormat,
    isUpdate,
    listStyle,
    linkUrlStyle,
    linkTitleStyle,
    currentFilePath,
    showToast,
  ]);

  const handleCopy = useCallback(() => {
    if (!code) return;
    const options = [
      isUpdate ? "-U" : "",
      inputFormat ? `-I ${inputFormat}` : "",
      listStyle ? `--list-style ${listStyle}` : "",
      linkUrlStyle ? `--link-url-style ${linkUrlStyle}` : "",
      linkTitleStyle ? `--link-title-style ${linkTitleStyle}` : "",
    ]
      .filter(Boolean)
      .join(" ");
    const script = `mq ${options} '${code}'`;

    navigator.clipboard
      .writeText(script)
      .then(() => {
        showToast("Command copied to clipboard!", "success");
      })
      .catch(() => {
        showToast("Failed to copy command", "error");
      });
  }, [
    code,
    inputFormat,
    isUpdate,
    listStyle,
    linkUrlStyle,
    linkTitleStyle,
    showToast,
  ]);

  const handleChangeListStyle = useCallback(
    (e: React.ChangeEvent<HTMLSelectElement>) => {
      const value = e.target.value;
      setListStyle(value as mq.Options["listStyle"]);
    },
    [],
  );

  const handleChangeLinkUrlStyle = useCallback(
    (e: React.ChangeEvent<HTMLSelectElement>) => {
      const value = e.target.value;
      setLinkUrlStyle(value as mq.Options["linkUrlStyle"]);
    },
    [],
  );

  const getFileType = (filePath: string): Tab["fileType"] => {
    const extension = filePath.split(".").pop()?.toLowerCase();
    if (extension === "mq") return "mq";
    if (extension === "md") return "md";
    if (extension === "mdx") return "mdx";
    return "other";
  };

  const generateTabId = () => `tab-${Date.now()}-${Math.random()}`;

  // Helper function to set editor content based on file type
  const setEditorContent = (filePath: string, content: string) => {
    setCode(content);
    setCurrentFilePath(filePath);

    const fileType = getFileType(filePath);
    if (fileType === "md" || fileType === "mdx") {
      setMarkdown(content);
      setInputFormat(fileType === "mdx" ? "mdx" : "markdown");
    }
  };

  const openOrSwitchToTab = useCallback((filePath: string, content: string) => {
    setTabs((prevTabs) => {
      // Check if tab already exists
      const existingTab = prevTabs.find((tab) => tab.filePath === filePath);
      if (existingTab) {
        setActiveTabId(existingTab.id);
        setEditorContent(filePath, existingTab.content);
        return prevTabs; // Don't modify tabs
      }

      // Create new tab
      const newTab: Tab = {
        id: generateTabId(),
        filePath,
        content,
        savedContent: content,
        fileType: getFileType(filePath),
        isDirty: false,
      };

      setActiveTabId(newTab.id);
      setEditorContent(filePath, content);

      return [...prevTabs, newTab];
    });
  }, []);

  const handleTabClick = useCallback(
    async (tabId: string) => {
      if (tabId === activeTabId) return;

      // Auto-save current tab before switching
      if (currentFilePath && code !== undefined && activeTabId) {
        const currentTab = tabs.find((t) => t.id === activeTabId);
        if (currentTab?.isDirty) {
          try {
            setSaveStatus("saving");
            await fileSystem.writeFile(currentFilePath, code);
            setSaveStatus("saved");
            setTabs((prev) =>
              prev.map((tab) =>
                tab.id === activeTabId
                  ? {
                      ...tab,
                      content: code,
                      savedContent: code,
                      isDirty: false,
                    }
                  : tab,
              ),
            );
          } catch (error) {
            console.error("Failed to auto-save file:", error);
            setSaveStatus("unsaved");
          }
        }
      }

      const tab = tabs.find((t) => t.id === tabId);
      if (!tab) return;

      setActiveTabId(tabId);
      setEditorContent(tab.filePath, tab.content);
      setSelectedFile(tab.filePath);
    },
    [tabs, activeTabId, currentFilePath, code],
  );

  const closeTab = useCallback(
    (tabId: string) => {
      const newTabs = tabs.filter((t) => t.id !== tabId);
      setTabs(newTabs);

      // If closing active tab, switch to another tab
      if (activeTabId === tabId) {
        if (newTabs.length > 0) {
          const nextTab = newTabs[newTabs.length - 1];
          setActiveTabId(nextTab.id);
          setEditorContent(nextTab.filePath, nextTab.content);
          setSelectedFile(nextTab.filePath);
        } else {
          setActiveTabId(null);
          setCurrentFilePath(null);
          setSelectedFile(null);
        }
      }
    },
    [tabs, activeTabId],
  );

  const handleTabClose = useCallback(
    (tabId: string) => {
      const tab = tabs.find((t) => t.id === tabId);
      if (!tab) return;

      // Check if tab has unsaved changes — show ConfirmDialog instead of window.confirm
      if (tab.isDirty) {
        setTabCloseConfirm({ tabId, filePath: tab.filePath });
        return;
      }

      closeTab(tabId);
    },
    [tabs, closeTab],
  );

  const handleFileSelect = useCallback(
    async (path: string) => {
      try {
        setSelectedFile(path);
        const content = await fileSystem.readFile(path);
        openOrSwitchToTab(path, content);
        localStorage.setItem(CURRENT_FILE_PATH_KEY, path);
        localStorage.setItem(SELECTED_FILE_KEY, path);
      } catch (error) {
        console.error("Failed to read file:", error);
        showToast(`Failed to read file: ${error}`, "error");
      }
    },
    [openOrSwitchToTab],
  );

  const handleCreateFile = useCallback(
    async (parentPath: string | undefined, fileName: string) => {
      if (!fileName || !fileName.trim()) {
        return;
      }

      try {
        const trimmedName = fileName.trim();
        const path = parentPath
          ? `${parentPath}/${trimmedName}`
          : `/${trimmedName}`;

        await fileSystem.writeFile(path, "");
        await loadFiles();
        openOrSwitchToTab(path, "");
        setSelectedFile(path);
        localStorage.setItem(CURRENT_FILE_PATH_KEY, path);
        localStorage.setItem(SELECTED_FILE_KEY, path);
      } catch (error) {
        console.error("Failed to create file:", error);
        showToast(
          `Failed to create file: ${
            error instanceof Error ? error.message : String(error)
          }`,
          "error",
        );
      }
    },
    [loadFiles, openOrSwitchToTab],
  );

  const handleCreateFolder = useCallback(
    async (parentPath: string | undefined, folderName: string) => {
      if (!folderName || !folderName.trim()) {
        return;
      }

      try {
        const trimmedName = folderName.trim();
        const path = parentPath
          ? `${parentPath}/${trimmedName}`
          : `/${trimmedName}`;

        await fileSystem.createDirectory(path);
        await loadFiles();
      } catch (error) {
        console.error("Failed to create folder:", error);
        showToast(
          `Failed to create folder: ${
            error instanceof Error ? error.message : String(error)
          }`,
          "error",
        );
      }
    },
    [loadFiles],
  );

  const handleDeleteFile = useCallback((path: string) => {
    setDeleteConfirmDialog({ path });
  }, []);

  const confirmDelete = useCallback(async () => {
    if (!deleteConfirmDialog) return;

    const path = deleteConfirmDialog.path;
    setDeleteConfirmDialog(null);

    try {
      // Check if it's a directory or file
      const parts = path.split("/").filter(Boolean);
      if (parts.length === 0) {
        showToast("Cannot delete root directory", "error");
        return;
      }

      // Try to delete as file first, if fails try as directory
      try {
        await fileSystem.deleteFile(path);
      } catch {
        await fileSystem.deleteDirectory(path);
      }

      await loadFiles();

      // Close tab if deleted file was open
      const deletedTab = tabs.find((tab) => tab.filePath === path);
      if (deletedTab) {
        handleTabClose(deletedTab.id);
      }

      // Clear editor if deleted file was currently selected
      if (selectedFile === path) {
        setSelectedFile(null);
        setCurrentFilePath(null);
        localStorage.removeItem(SELECTED_FILE_KEY);
        localStorage.removeItem(CURRENT_FILE_PATH_KEY);
      }
    } catch (error) {
      console.error("Failed to delete:", error);
      showToast("Failed to delete", "error");
    }
  }, [deleteConfirmDialog, loadFiles, selectedFile, tabs, handleTabClose]);

  const handleRenameFile = useCallback(
    async (path: string, newName: string) => {
      const parts = path.split("/").filter(Boolean);
      const currentName = parts[parts.length - 1];

      const trimmedNewName = newName.trim();

      if (!trimmedNewName || trimmedNewName === currentName) {
        return;
      }

      // Validate filename
      if (trimmedNewName.includes("/")) {
        showToast("Filename cannot contain '/'", "error");
        return;
      }

      setIsRenaming(true);
      try {
        // Check if we're trying to convert between file and directory
        const isDirectory = await fileSystem.isDirectoryPath(path);
        const hasExtension = trimmedNewName.includes(".");
        const currentHasExtension = currentName.includes(".");

        // Prevent converting directory to file or vice versa
        if (isDirectory && hasExtension && !currentHasExtension) {
          showToast(
            "Cannot rename a folder to a file name. Folders cannot have file extensions.",
            "error",
          );
          setIsRenaming(false);
          return;
        }

        const parentPath = parts.slice(0, -1).join("/");
        const newPath = parentPath
          ? `/${parentPath}/${trimmedNewName}`
          : `/${trimmedNewName}`;

        let savedContent: string | null = null;
        const isCurrentFile = currentFilePath === path;

        if (isCurrentFile && code) {
          // Save the current file before renaming to ensure it's written
          savedContent = code;
          await fileSystem.writeFile(path, code);
          setSaveStatus("saved");

          // Close the file to release the handle
          setCurrentFilePath(null);
          setSelectedFile(null);

          // Rename with the saved content
          await fileSystem.renameFile(path, newPath, savedContent);
        } else {
          // For non-current files, just rename normally
          await fileSystem.renameFile(path, newPath);
        }
        await loadFiles();

        // Update tab path if the file is open in a tab
        setTabs((prev) =>
          prev.map((tab) =>
            tab.filePath === path ? { ...tab, filePath: newPath } : tab,
          ),
        );

        // If this was the current file, reopen it with the new path
        if (isCurrentFile && savedContent !== null) {
          setCurrentFilePath(newPath);
          setSelectedFile(newPath);
          setCode(savedContent);
          localStorage.setItem(CURRENT_FILE_PATH_KEY, newPath);
          localStorage.setItem(SELECTED_FILE_KEY, newPath);
        } else if (selectedFile === path) {
          setSelectedFile(newPath);
          localStorage.setItem(SELECTED_FILE_KEY, newPath);
        }
      } catch (error) {
        console.error("Failed to rename:", error);
        showToast(
          `Failed to rename: ${
            error instanceof Error ? error.message : String(error)
          }`,
          "error",
        );
        // Reload files to ensure UI is in sync
        await loadFiles();
      } finally {
        setIsRenaming(false);
      }
    },
    [loadFiles, selectedFile, currentFilePath, code],
  );

  const handleMoveFile = useCallback(
    async (sourcePath: string, targetPath: string) => {
      try {
        // Get the filename from the source path
        const sourceParts = sourcePath.split("/").filter(Boolean);
        const fileName = sourceParts[sourceParts.length - 1];

        // Construct the new path (handle root case where targetPath is empty)
        const newPath = targetPath
          ? `${targetPath}/${fileName}`
          : `/${fileName}`;

        // Check if a file/folder with the same name already exists at the target
        const exists = await fileSystem.fileExists(newPath);
        if (exists) {
          const targetLocation = targetPath ? targetPath : "root";
          showToast(
            `A file or folder named "${fileName}" already exists in "${targetLocation}"`,
            "error",
          );
          return;
        }

        // Check if source is a directory
        const isDirectory = await fileSystem.isDirectoryPath(sourcePath);
        const isCurrentFile = currentFilePath === sourcePath;
        let savedContent: string | null = null;

        if (isCurrentFile && code && !isDirectory) {
          // Save the current file before moving
          savedContent = code;
          await fileSystem.writeFile(sourcePath, code);
          setSaveStatus("saved");

          // Close the file to release the handle
          setCurrentFilePath(null);
          setSelectedFile(null);

          // Move the file
          await fileSystem.renameFile(sourcePath, newPath, savedContent);
        } else {
          // For non-current files or directories, just move normally
          await fileSystem.renameFile(sourcePath, newPath);
        }

        await loadFiles();

        // Update tab path if the file is open in a tab
        setTabs((prev) =>
          prev.map((tab) =>
            tab.filePath === sourcePath ? { ...tab, filePath: newPath } : tab,
          ),
        );

        // If this was the current file, reopen it with the new path
        if (isCurrentFile && savedContent !== null) {
          setCurrentFilePath(newPath);
          setSelectedFile(newPath);
          setCode(savedContent);
          localStorage.setItem(CURRENT_FILE_PATH_KEY, newPath);
          localStorage.setItem(SELECTED_FILE_KEY, newPath);
        } else if (selectedFile === sourcePath) {
          setSelectedFile(newPath);
          localStorage.setItem(SELECTED_FILE_KEY, newPath);
        }
      } catch (error) {
        console.error("Failed to move:", error);
        showToast(
          `Failed to move: ${
            (error instanceof Error ? error.message : String(error), "error")
          }`,
        );
        // Reload files to ensure UI is in sync
        await loadFiles();
      }
    },
    [loadFiles, selectedFile, currentFilePath, code],
  );

  const saveCurrentFile = useCallback(async () => {
    if (!currentFilePath || !code || !activeTabId) return;

    try {
      setSaveStatus("saving");
      await fileSystem.writeFile(currentFilePath, code);
      setSaveStatus("saved");

      // Update tab to mark as not dirty and update savedContent
      setTabs((prev) =>
        prev.map((tab) =>
          tab.id === activeTabId
            ? { ...tab, content: code, savedContent: code, isDirty: false }
            : tab,
        ),
      );
    } catch (error) {
      console.error("Failed to save file:", error);
      setSaveStatus("unsaved");
    }
  }, [currentFilePath, code, activeTabId]);

  // Keep vim-accessible refs in sync with latest callbacks and state
  useEffect(() => {
    saveCurrentFileRef.current = saveCurrentFile;
  }, [saveCurrentFile]);

  useEffect(() => {
    activeTabIdRef.current = activeTabId;
  }, [activeTabId]);

  useEffect(() => {
    handleTabCloseRef.current = handleTabClose;
  }, [handleTabClose]);

  // Register vim Ex commands once on mount
  useEffect(() => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const Vim = (VimMode as any).Vim;
    if (!Vim) return;

    Vim.defineEx("write", "w", () => {
      saveCurrentFileRef.current();
    });

    Vim.defineEx("wq", "wq", () => {
      saveCurrentFileRef.current().then(() => {
        if (activeTabIdRef.current) {
          handleTabCloseRef.current(activeTabIdRef.current);
        }
      });
    });

    Vim.defineEx("quit", "q", () => {
      if (activeTabIdRef.current) {
        handleTabCloseRef.current(activeTabIdRef.current);
      }
    });

    Vim.defineEx("qall", "qa", () => {
      if (activeTabIdRef.current) {
        handleTabCloseRef.current(activeTabIdRef.current);
      }
    });
  }, []);

  // Track unsaved changes and update tab content and dirty state
  useEffect(() => {
    if (currentFilePath && code !== undefined && !isRenaming && activeTabId) {
      // Update current tab's content and dirty state
      setTabs((prev) =>
        prev.map((tab) => {
          if (tab.id === activeTabId) {
            // Check if content has changed from the saved version
            const isDirty = tab.savedContent !== code;
            if (isDirty) {
              setSaveStatus("unsaved");
            }
            return { ...tab, content: code, isDirty };
          }
          return tab;
        }),
      );
    }
  }, [code, currentFilePath, isRenaming, activeTabId]);

  // Persist sidebar visibility to localStorage
  useEffect(() => {
    localStorage.setItem(
      "mq-playground.sidebar-visible",
      String(isSidebarVisible),
    );
  }, [isSidebarVisible]);

  useEffect(() => {
    saveEditorSettings({
      vimModeEnabled,
      fontSize,
      theme,
      lineNumbers,
      tabSize,
      minimapEnabled,
      wordWrap,
      shareMode,
    });

    if (theme === "system") {
      document.documentElement.style.colorScheme = "";
      document.documentElement.removeAttribute("data-theme");
      document.documentElement.style.removeProperty("--lightningcss-light");
      document.documentElement.style.removeProperty("--lightningcss-dark");
    } else if (theme === "dark") {
      document.documentElement.style.colorScheme = "dark";
      document.documentElement.removeAttribute("data-theme");
      document.documentElement.style.setProperty("--lightningcss-light", " ");
      document.documentElement.style.setProperty(
        "--lightningcss-dark",
        "initial",
      );
    } else if (theme === "mq") {
      document.documentElement.style.colorScheme = "dark";
      document.documentElement.setAttribute("data-theme", "mq");
      document.documentElement.style.setProperty("--lightningcss-light", " ");
      document.documentElement.style.setProperty(
        "--lightningcss-dark",
        "initial",
      );
    } else {
      document.documentElement.style.colorScheme = "light";
      document.documentElement.removeAttribute("data-theme");
      document.documentElement.style.setProperty(
        "--lightningcss-light",
        "initial",
      );
      document.documentElement.style.setProperty("--lightningcss-dark", " ");
    }
  }, [
    vimModeEnabled,
    fontSize,
    theme,
    lineNumbers,
    tabSize,
    minimapEnabled,
    wordWrap,
    shareMode,
  ]);

  // Add keyboard shortcut for save (Ctrl+S / Cmd+S)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        saveCurrentFile();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [saveCurrentFile]);

  const toggleSidebar = useCallback(() => {
    setIsSidebarVisible((prev) => !prev);
  }, []);

  useEffect(() => {
    if (editorRef.current) {
      if (vimModeEnabled) {
        if (!vimModeRef.current) {
          vimModeRef.current = initVimMode(
            editorRef.current,
            vimStatusBarRef.current,
          );
        }
      } else {
        if (vimModeRef.current) {
          vimModeRef.current.dispose();
          vimModeRef.current = null;
          if (vimStatusBarRef.current) {
            vimStatusBarRef.current.innerHTML = "";
          }
        }
      }
    }
  }, [vimModeEnabled]);

  const handleSidebarResize = useCallback((delta: number) => {
    setSidebarWidth((prev) => {
      const next = Math.min(500, Math.max(150, prev + delta));
      localStorage.setItem(SIDEBAR_WIDTH_KEY, String(next));
      return next;
    });
  }, []);

  const handlePanelResize = useCallback(
    (delta: number) => {
      const container = contentRef.current;
      if (!container) return;
      const availableWidth =
        container.clientWidth - (isSidebarVisible ? sidebarWidth : 0);
      if (availableWidth <= 0) return;
      const deltaPct = (delta / availableWidth) * 100;
      setLeftRightSplit((prev) => {
        const next = Math.min(80, Math.max(20, prev + deltaPct));
        localStorage.setItem(LEFT_RIGHT_SPLIT_KEY, String(next));
        return next;
      });
    },
    [isSidebarVisible, sidebarWidth],
  );

  const handleEditorResize = useCallback((delta: number) => {
    const leftPanel = leftPanelRef.current;
    if (!leftPanel) return;
    const height = leftPanel.clientHeight;
    if (height <= 0) return;
    const deltaPct = (delta / height) * 100;
    setTopBottomSplit((prev) => {
      const next = Math.min(80, Math.max(20, prev + deltaPct));
      localStorage.setItem(TOP_BOTTOM_SPLIT_KEY, String(next));
      return next;
    });
  }, []);

  const handleProblemsPanelResize = useCallback((delta: number) => {
    setProblemsPanelHeight((prev) => {
      const next = Math.max(80, Math.min(500, prev - delta));
      localStorage.setItem(PROBLEMS_PANEL_HEIGHT_KEY, String(next));
      return next;
    });
  }, []);

  const toggleMinimap = useCallback(() => {
    setMinimapEnabled((prev) => !prev);
  }, []);

  const toggleWordWrap = useCallback(() => {
    setWordWrap((prev) => (prev === "on" ? "off" : "on"));
  }, []);

  const beforeMount = (monaco: Monaco) => {
    monaco.editor.addEditorAction({
      id: "run-script",
      label: "Run Script",
      keybindings: [monaco.KeyMod.WinCtrl | monaco.KeyCode.Enter],
      run: () => {
        handleRun();
      },
    });

    monaco.editor.onDidCreateEditor((editorInstance: editor.ICodeEditor) => {
      editorInstance.onDidChangeModelContent(() => {
        if (diagnosticsTimerRef.current) {
          clearTimeout(diagnosticsTimerRef.current);
        }
        diagnosticsTimerRef.current = setTimeout(async () => {
          const model = editorInstance.getModel();
          if (model) {
            const modelLanguage = model.getLanguageId();

            if (modelLanguage === "markdown" || modelLanguage === "json") {
              return;
            }

            const errors = await mq.diagnostics(
              model.getValue(),
              enableTypeCheckRef.current,
            );
            const markers = errors.map((error: mq.Diagnostic) => ({
              startLineNumber: error.startLine,
              startColumn: error.startColumn,
              endLineNumber: error.endLine,
              endColumn: error.endColumn,
              message: error.message,
              severity: monaco.MarkerSeverity.Error,
            }));
            monaco.editor.setModelMarkers(model, "mq", markers);
            setDiagnostics({
              errors: markers.filter(
                (m) => m.severity === monaco.MarkerSeverity.Error,
              ).length,
              warnings: markers.filter(
                (m) => m.severity === monaco.MarkerSeverity.Warning,
              ).length,
              items: markers,
            });
          }
        }, 300);
      });
    });

    monaco.languages.registerCompletionItemProvider("mq", {
      triggerCharacters: ["|", ":", "."],
      provideCompletionItems: async (
        model: editor.ITextModel,
        position: IPosition,
      ) => {
        const wordRange = model.getWordUntilPosition(position);

        let moduleName: string | undefined = undefined;
        const lineContent = model.getLineContent(position.lineNumber);
        const uptoCursor = lineContent.slice(0, position.column - 1);
        const moduleMatch = uptoCursor.match(/([a-zA-Z_][\w]*)::/);
        if (moduleMatch) {
          moduleName = moduleMatch[1];
        }

        const values = await mq.definedValues(model.getValue(), moduleName);
        const suggestions: languages.CompletionItem[] = values.map(
          (value: mq.DefinedValue) => {
            return {
              label: value.name,
              kind:
                value.valueType === "Function"
                  ? monaco.languages.CompletionItemKind.Function
                  : value.valueType === "Variable"
                    ? monaco.languages.CompletionItemKind.Variable
                    : value.valueType === "Selector"
                      ? monaco.languages.CompletionItemKind.Method
                      : monaco.languages.CompletionItemKind.Property,
              insertText:
                value.valueType === "Function"
                  ? `${value.name}(${
                      value.args
                        ?.map((name: string, i: number) => `$\{${i}:${name}}`)
                        .join(", ") || ""
                    })`
                  : value.name,
              insertTextRules:
                monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
              detail: value.doc,
              documentation: value.doc,
              range: {
                startLineNumber: position.lineNumber,
                startColumn: wordRange.startColumn,
                endLineNumber: position.lineNumber,
                endColumn: wordRange.endColumn,
              },
            };
          },
        );

        const snippets: languages.CompletionItem[] = [
          {
            label: "foreach",
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: "foreach (${1:item}, ${2:items}): ${0:body};",
            insertTextRules:
              monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: "Loop over each item in a collection",
            documentation:
              "Creates a foreach loop to iterate through items in a collection",
            range: {
              startLineNumber: position.lineNumber,
              startColumn: wordRange.startColumn,
              endLineNumber: position.lineNumber,
              endColumn: wordRange.endColumn,
            },
          },
          {
            label: "include",
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: 'include "${1:path}"',
            insertTextRules:
              monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: "Include a module",
            documentation:
              "Includes the contents of an external module for processing.",
            range: {
              startLineNumber: position.lineNumber,
              startColumn: wordRange.startColumn,
              endLineNumber: position.lineNumber,
              endColumn: wordRange.endColumn,
            },
          },
          {
            label: "while",
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: "while (${1:condition}): ${0:body};",
            insertTextRules:
              monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: "Loop while condition is true",
            documentation:
              "Creates a while loop that continues execution as long as condition is true",
            range: {
              startLineNumber: position.lineNumber,
              startColumn: wordRange.startColumn,
              endLineNumber: position.lineNumber,
              endColumn: wordRange.endColumn,
            },
          },
          {
            label: "def",
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: "def ${0}(${1:args}): ${2:body};",
            insertTextRules:
              monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: "Define a custom function",
            documentation:
              "Creates a reusable function with custom parameters that can be called elsewhere in the script",
            range: {
              startLineNumber: position.lineNumber,
              startColumn: wordRange.startColumn,
              endLineNumber: position.lineNumber,
              endColumn: wordRange.endColumn,
            },
          },
          {
            label: "fn",
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: "fn(${0:args}): ${1:body};",
            insertTextRules:
              monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: "Define an anonymous function",
            documentation:
              "Creates an anonymous function that can be used inline without naming it",
            range: {
              startLineNumber: position.lineNumber,
              startColumn: wordRange.startColumn,
              endLineNumber: position.lineNumber,
              endColumn: wordRange.endColumn,
            },
          },
          {
            label: "match",
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: "match (${1:value}):\n  | ${2:pattern}: ${3:body}\nend",
            insertTextRules:
              monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: "Match expression",
            documentation:
              "Creates a match expression that destructures the value and matches it against patterns",
            range: {
              startLineNumber: position.lineNumber,
              startColumn: wordRange.startColumn,
              endLineNumber: position.lineNumber,
              endColumn: wordRange.endColumn,
            },
          },
          {
            label: "module",
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: "module ${1:name}:\n  ${2:body}\nend",
            insertTextRules:
              monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: "Module declaration",
            documentation:
              "Creates a module that contains a set of related functions and variables.",
            range: {
              startLineNumber: position.lineNumber,
              startColumn: wordRange.startColumn,
              endLineNumber: position.lineNumber,
              endColumn: wordRange.endColumn,
            },
          },
          {
            label: "macro",
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: "macro ${1:name}(${2:args}) do\n  ${3:body}:\nend",
            insertTextRules:
              monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: "Macro declaration",
            documentation:
              "Creates a macro that generates code at compile time for reuse.",
            range: {
              startLineNumber: position.lineNumber,
              startColumn: wordRange.startColumn,
              endLineNumber: position.lineNumber,
              endColumn: wordRange.endColumn,
            },
          },
          {
            label: "quote",
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: "quote do\n  ${1:body}:\nend",
            insertTextRules:
              monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: "Quote block",
            documentation:
              "Creates a quote block that treats the code inside as data rather than executing it.",
            range: {
              startLineNumber: position.lineNumber,
              startColumn: wordRange.startColumn,
              endLineNumber: position.lineNumber,
              endColumn: wordRange.endColumn,
            },
          },
          {
            label: "unquote",
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: "unquote(${1:expr})",
            insertTextRules:
              monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: "Unquote expression",
            documentation:
              "Inserts the result of an expression into a quote block.",
            range: {
              startLineNumber: position.lineNumber,
              startColumn: wordRange.startColumn,
              endLineNumber: position.lineNumber,
              endColumn: wordRange.endColumn,
            },
          },
        ];

        return { suggestions: [...suggestions, ...snippets] };
      },
    });

    monaco.languages.registerHoverProvider("mq", {
      provideHover: async (model: editor.ITextModel, position: IPosition) => {
        const result = await mq.hover(
          model.getValue(),
          position.lineNumber,
          position.column,
        );
        if (!result) return null;
        return {
          contents: [
            { value: result.content, isTrusted: true, supportThemeIcons: true },
          ],
        };
      },
    });

    monaco.languages.registerInlayHintsProvider("mq", {
      provideInlayHints: async (model: editor.ITextModel) => {
        if (!enableTypeCheckRef.current) {
          return { hints: [], dispose: () => {} };
        }
        const hints = await mq.inlayHints(model.getValue());
        return {
          hints: hints.map((hint: mq.InlayHint) => ({
            position: {
              lineNumber: hint.line,
              column: hint.column,
            },
            label: hint.label,
            kind: monaco.languages.InlayHintKind.Type,
          })),
          dispose: () => {},
        };
      },
    });

    monaco.languages.register({ id: "mq" });
    monaco.languages.setMonarchTokensProvider("mq", {
      tokenizer: {
        root: [
          [/#[^\n]*/, "comment"],
          [
            /\b(let|def|do|match|while|foreach|if|elif|else|end|self|None|nodes|break|continue|include|import|module|var|macro|quote|unquote|loop)\b/,
            "keyword",
          ],
          [/;/, "delimiter"],
          [
            /(\/\/=|<<|>>|\|\||\?\?|<=|>=|==|!=|=~|&&|\+=|-=|\*=|\/=|\|=|=|\||:|;|\?|!|\+|-|\*|\/|%|<|>|@)/,
            "operator",
          ],
          [/"/, { token: "string", next: "@multilineString" }],
          [/"s"/, { token: "string", next: "@multilineString" }],
          [/\d+/, "number"],
          [/[a-zA-Z_]\w*(?=\s*\()/, "function"],
          [/(([a-zA-Z_]\w*)\s*\()/, "function"],
          [/\(|\)|\[|\]/, "delimiter.parenthesis"],
          [/[a-zA-Z_]\w*/, "variable"],
          [/:[a-zA-Z_][a-zA-Z0-9_]*/, "variable"],
        ],
        multilineString: [
          [/\$\{[^}]*\}/, "variable"],
          [/\\./, "string.escape"], // handle escaped characters (including \" )
          [/[^\\"]+/, "string"], // match all except backslash and quote
          [/"/, { token: "string", next: "@pop" }],
        ],
      },
      unicode: true,
      includeLF: true,
    });

    const rules = (dark: boolean) => [
      {
        token: "comment",
        foreground: dark ? "#6A9955" : "#008000",
        fontStyle: "italic",
      },
      {
        token: "keyword",
        foreground: dark ? "#569CD6" : "#0000FF",
        fontStyle: "bold",
      },
      { token: "function", foreground: dark ? "#DCDCAA" : "#795E26" },
      { token: "variable", foreground: dark ? "#9CDCFE" : "#001080" },
      { token: "property", foreground: dark ? "#9CDCFE" : "#001080" },
      { token: "string", foreground: dark ? "#CE9178" : "#A31515" },
      { token: "number", foreground: dark ? "#B5CEA8" : "#098658" },
      {
        token: "operator",
        foreground: dark ? "#D4D4D4" : "#000000",
        fontStyle: "bold",
      },
      { token: "delimiter", foreground: dark ? "#D4D4D4" : "#000000" },
      { token: "identifier", foreground: dark ? "#D4D4D4" : "#000000" },
    ];

    monaco.editor.defineTheme("mq-dark", {
      base: "vs-dark",
      inherit: true,
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      rules: rules(true) as any,
      colors: {
        "editor.background": "#1E1E1E",
        "editor.foreground": "#D4D4D4",
        "editorLineNumber.foreground": "#858585",
        "editor.lineHighlightBackground": "#2D2D30",
        "editorCursor.foreground": "#A7A7A7",
      },
    });

    monaco.editor.defineTheme("mq-light", {
      base: "vs",
      inherit: true,
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      rules: rules(false) as any,
      colors: {
        "editor.background": "#FFFFFF",
        "editor.foreground": "#000000",
        "editorLineNumber.foreground": "#237893",
        "editor.lineHighlightBackground": "#F3F3F3",
        "editorCursor.foreground": "#000000",
      },
    });

    monaco.editor.defineTheme("mq-branded", {
      base: "vs-dark",
      inherit: true,
      rules: [
        { token: "comment", foreground: "#6b7a90", fontStyle: "italic" },
        { token: "keyword", foreground: "#67b8e3", fontStyle: "bold" },
        { token: "function", foreground: "#56d4d4" },
        { token: "variable", foreground: "#9cdcfe" },
        { token: "property", foreground: "#67e8f9" },
        { token: "string", foreground: "#89ddff" },
        { token: "string.escape", foreground: "#67e8f9" },
        { token: "number", foreground: "#de935f" },
        { token: "operator", foreground: "#94a3b8" },
        { token: "delimiter", foreground: "#94a3b8" },
        { token: "delimiter.parenthesis", foreground: "#94a3b8" },
        { token: "identifier", foreground: "#e2e8f0" },
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
      ] as any,
      colors: {
        "editor.background": "#1e293b",
        "editor.foreground": "#e2e8f0",
        "editorLineNumber.foreground": "#4a5568",
        "editor.lineHighlightBackground": "#2a3444",
        "editorCursor.foreground": "#67b8e3",
        "editorIndentGuide.background1": "#2a3444",
        "editorIndentGuide.activeBackground1": "#4a5568",
        "editor.selectionBackground": "#32404f",
        "editor.inactiveSelectionBackground": "#2a3444",
      },
    });
  };

  const isDarkMode =
    theme === "system"
      ? window.matchMedia("(prefers-color-scheme: dark)").matches
      : theme === "dark" || theme === "mq";

  const monacoTheme =
    theme === "mq" ? "mq-branded" : isDarkMode ? "mq-dark" : "mq-light";

  const buildPreviewSrcDoc = (htmlFragment: string) => {
    const resolvedTheme =
      theme === "system"
        ? window.matchMedia("(prefers-color-scheme: dark)").matches
          ? "dark"
          : "light"
        : theme;

    const colors =
      resolvedTheme === "mq"
        ? {
            bg: "#1e293b",
            fg: "#e2e8f0",
            preBg: "#2a3444",
            link: "#67b8e3",
            heading: "#e2e8f0",
            border: "#32404f",
          }
        : resolvedTheme === "dark"
          ? {
              bg: "#1e1e1e",
              fg: "#d4d4d4",
              preBg: "#2d2d2d",
              link: "#4ec9b0",
              heading: "#d4d4d4",
              border: "#3e3e42",
            }
          : {
              bg: "#ffffff",
              fg: "#1a1a1a",
              preBg: "#f5f5f5",
              link: "#0070c1",
              heading: "#1a1a1a",
              border: "#e0e0e0",
            };

    return `<!DOCTYPE html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data: blob:;"><style>
body{margin:16px;font-family:sans-serif;background:${colors.bg};color:${colors.fg};line-height:1.6}
h1,h2,h3,h4,h5,h6{color:${colors.heading};border-bottom:1px solid ${colors.border};padding-bottom:0.3em}
a{color:${colors.link}}
pre{background:${colors.preBg};padding:12px;border-radius:4px;overflow:auto;border:1px solid ${colors.border}}
code{font-family:'JetBrains Mono',monospace;font-size:0.9em}
blockquote{border-left:4px solid ${colors.border};margin:0;padding:0 1em;color:${colors.fg};opacity:0.8}
table{border-collapse:collapse;width:100%}
th,td{border:1px solid ${colors.border};padding:6px 12px}
th{background:${colors.preBg}}
img{max-width:100%}
</style></head><body>${htmlFragment}</body></html>`;
  };

  useEffect(() => {
    const handleVisibilityChange = () => {
      if (document.hidden) {
        localStorage.setItem(CODE_KEY, code || "");
        localStorage.setItem(MARKDOWN_KEY, markdown || "");
        localStorage.setItem(IS_UPDATE_KEY, String(isUpdate));
        localStorage.setItem(ENABLE_TYPE_CHECK_KEY, String(enableTypeCheck));
        localStorage.setItem(INPUT_FORMAT_KEY, inputFormat || "markdown");
      }
    };

    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () => {
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, [code, markdown, isUpdate, enableTypeCheck, inputFormat]);

  const isDesktopView = isDesktop();

  const previewSrcDoc = previewHtml
    ? buildPreviewSrcDoc(previewHtml)
    : buildPreviewSrcDoc(
        "<p style='color:#888'>Click \"Run\" button to display preview</p>",
      );

  return (
    <div className="playground-container">
      {!isEmbed && (
        <header className="playground-header">
          <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
            {isOPFSSupported && (
              <button
                className="header-icon-button"
                onClick={toggleSidebar}
                title={isSidebarVisible ? "Hide Sidebar" : "Show Sidebar"}
              >
                {isSidebarVisible ? (
                  <VscLayoutSidebarLeft size={16} />
                ) : (
                  <VscLayoutSidebarLeftOff size={16} />
                )}
              </button>
            )}
            <a
              href="https://mqlang.org/"
              style={{
                textDecoration: "none",
                paddingTop: "6px",
              }}
              target="_blank"
            >
              <img src="./logo.svg" className="logo-icon" />
            </a>
            <h1 style={{ color: "var(--header-title-color)" }}>playground</h1>
          </div>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: "12px",
              marginRight: "8px",
            }}
          >
            <button
              className="header-icon-button"
              onClick={() => setIsSettingsOpen(true)}
              title="Settings"
            >
              <VscSettingsGear size={16} />
            </button>
            <a
              href="https://github.com/harehare/mq"
              target="_blank"
              rel="noopener noreferrer"
            >
              <img
                src="https://img.shields.io/github/stars/harehare/mq?style=social"
                alt="GitHub stars"
              />
            </a>
          </div>
        </header>
      )}

      <div className="playground-content" ref={contentRef}>
        {isOPFSSupported && isSidebarVisible && !isEmbed && (
          <>
            <div
              className="file-tree-panel"
              style={isDesktopView ? { width: sidebarWidth } : undefined}
            >
              <FileTree
                files={files}
                onFileSelect={handleFileSelect}
                onRefresh={loadFiles}
                onCreateFile={handleCreateFile}
                onCreateFolder={handleCreateFolder}
                onDeleteFile={handleDeleteFile}
                onRenameFile={handleRenameFile}
                onMoveFile={handleMoveFile}
                selectedFile={selectedFile}
              />
            </div>
            {isDesktopView && (
              <ResizeHandle
                direction="horizontal"
                onResize={handleSidebarResize}
              />
            )}
          </>
        )}
        <div
          className="left-panel"
          ref={leftPanelRef}
          style={
            isDesktopView
              ? {
                  width: `calc((100% - ${isOPFSSupported && isSidebarVisible ? sidebarWidth + 4 : 0}px) * ${leftRightSplit / 100})`,
                  flex: "none",
                }
              : undefined
          }
        >
          <div
            className="editor-container"
            style={isDesktopView ? { height: `${topBottomSplit}%` } : undefined}
          >
            {!isEmbed && tabs.length > 0 && (
              <TabBar
                tabs={tabs}
                activeTabId={activeTabId}
                onTabClick={handleTabClick}
                onTabClose={handleTabClose}
              />
            )}
            <div className="editor-header code">
              <h2>Code</h2>
              <div className="editor-actions">
                <button
                  className="button"
                  onClick={() => setIsExamplesOpen(true)}
                  title="Browse examples"
                >
                  <VscListUnordered size={14} />
                  <span>Examples</span>
                </button>
                <button
                  className="button"
                  onClick={handleCopy}
                  title="Copy command"
                >
                  <VscCopy size={14} />
                  <span>Copy</span>
                </button>
                <button
                  className="button"
                  onClick={handleShare}
                  title="Share playground"
                >
                  <VscLinkExternal size={14} />
                  <span>Share</span>
                </button>
                <button
                  className="button format-button"
                  onClick={handleFormat}
                  title="Format code"
                >
                  <VscSymbolMethod size={14} />
                  <span>Format</span>
                </button>
                <button
                  className="button run-button"
                  onClick={handleRun}
                  title="Run script (Ctrl+Enter)"
                >
                  <VscPlay size={14} color="rgb(76, 175, 80)" />
                  <span>Run</span>
                </button>
              </div>
            </div>
            <div className="editor-content">
              <Editor
                className="editor"
                defaultLanguage="mq"
                value={code}
                onChange={setCode}
                beforeMount={beforeMount}
                onMount={(editor) => {
                  editorRef.current = editor as editor.IStandaloneCodeEditor;
                  const disposable = editor.onDidChangeCursorPosition((e) => {
                    setCursorPosition({
                      line: e.position.lineNumber,
                      column: e.position.column,
                    });
                  });

                  if (vimModeEnabled && !vimModeRef.current) {
                    vimModeRef.current = initVimMode(
                      editor,
                      vimStatusBarRef.current,
                    );
                  }

                  editor.onDidDispose(() => {
                    disposable.dispose();
                    if (vimModeRef.current) {
                      vimModeRef.current.dispose();
                      vimModeRef.current = null;
                    }
                    editorRef.current = null;
                  });
                }}
                options={{
                  minimap: { enabled: minimapEnabled },
                  scrollBeyondLastLine: false,
                  fontSize,
                  automaticLayout: true,
                  fontFamily:
                    "'JetBrains Mono', 'Source Code Pro', Menlo, monospace",
                  fontLigatures: true,
                  wordWrap,
                  lineNumbers,
                  tabSize,
                }}
                theme={monacoTheme}
              />
            </div>
          </div>

          {isDesktopView && (
            <ResizeHandle direction="vertical" onResize={handleEditorResize} />
          )}

          <div className="editor-container" style={{ flex: 1 }}>
            <div className="editor-header">
              <label className="label">
                <select
                  className="dropdown"
                  value={inputFormat || "markdown"}
                  onChange={(e) =>
                    setInputFormat(e.target.value as mq.Options["inputFormat"])
                  }
                >
                  <option value="markdown">Markdown</option>
                  <option value="mdx">MDX</option>
                  <option value="text">Text</option>
                  <option value="html">HTML</option>
                  <option value="null">NULL</option>
                  <option value="raw">Raw</option>
                </select>
              </label>
            </div>
            <div className="editor-content">
              <Editor
                height="100%"
                defaultLanguage="markdown"
                value={markdown}
                onChange={setMarkdown}
                options={{
                  minimap: { enabled: false },
                  scrollBeyondLastLine: false,
                  fontSize,
                  automaticLayout: true,
                  fontFamily:
                    "'JetBrains Mono', 'Source Code Pro', Menlo, monospace",
                  fontLigatures: true,
                  lineNumbers,
                  tabSize,
                }}
                theme={monacoTheme}
              />
            </div>
          </div>
        </div>
        {isDesktopView && (
          <ResizeHandle direction="horizontal" onResize={handlePanelResize} />
        )}
        <div className="right-panel">
          <div className="tab-container">
            <button
              className={`tab ${activeTab === "output" ? "active" : ""}`}
              onClick={() => setActiveTab("output")}
            >
              Output
            </button>
            <button
              className={`tab ${activeTab === "preview" ? "active" : ""}`}
              onClick={async () => {
                try {
                  setPreviewHtml(await toHtml(result));
                } catch {
                  setPreviewHtml("");
                }
                setActiveTab("preview");
              }}
            >
              Preview
            </button>
            <button
              className={`tab ${activeTab === "ast" ? "active" : ""}`}
              onClick={() => {
                handleGenerateAst();
                setActiveTab("ast");
              }}
            >
              AST
            </button>
          </div>
          {!isEmbed && (
            <div className="editor-header output">
              <div className="editor-actions">
                {activeTab === "output" && (
                  <>
                    <label className="label">
                      <div
                        style={{
                          marginRight: "4px",
                        }}
                      >
                        List Style:
                      </div>
                      <select
                        className="dropdown"
                        onChange={handleChangeListStyle}
                      >
                        <option value="dash">-</option>
                        <option value="star">*</option>
                        <option value="plus">+</option>
                      </select>
                    </label>
                    <label className="label">
                      <div
                        style={{
                          marginRight: "4px",
                        }}
                      >
                        URL Style:
                      </div>
                      <select
                        className="dropdown"
                        onChange={handleChangeLinkUrlStyle}
                      >
                        <option value="none">None</option>
                        <option value="angle">Angle</option>
                      </select>
                    </label>
                    <label className="label">
                      <div
                        style={{
                          marginRight: "4px",
                        }}
                      >
                        Title Style:
                      </div>
                      <select
                        className="dropdown"
                        onChange={(e) => {
                          const value = e.target.value;
                          const linkTitleStyle =
                            value === "none"
                              ? null
                              : (value as mq.Options["linkTitleStyle"]);
                          setLinkTitleStyle(linkTitleStyle);
                        }}
                      >
                        <option value="none">None</option>
                        <option value="double">Double</option>
                        <option value="single">Single</option>
                        <option value="paren">Paren</option>
                      </select>
                    </label>
                    <div>
                      <label className="label">
                        <input
                          type="checkbox"
                          checked={isUpdate}
                          onChange={(e) => setIsUpdate(e.target.checked)}
                          style={{
                            marginRight: "5px",
                            cursor: "pointer",
                          }}
                        />
                        <div>Update Markdown</div>
                      </label>
                    </div>
                    <div>
                      <label className="label">
                        <input
                          type="checkbox"
                          checked={enableTypeCheck}
                          onChange={(e) => setEnableTypeCheck(e.target.checked)}
                          style={{
                            marginRight: "5px",
                            cursor: "pointer",
                          }}
                        />
                        <div>Type Check (β)</div>
                      </label>
                    </div>
                  </>
                )}
                {activeTab === "ast" && (
                  <button className="button" onClick={handleGenerateAst}>
                    <VscSymbolMethod size={14} />
                    <span>Generate AST</span>
                  </button>
                )}
              </div>
            </div>
          )}
          <div className="editor-content result-container">
            {activeTab === "output" && (
              <Editor
                height="100%"
                defaultLanguage="markdown"
                defaultValue={`Click "Run" button to display results`}
                value={result}
                options={{
                  readOnly: true,
                  domReadOnly: true,
                  minimap: { enabled: false },
                  scrollBeyondLastLine: false,
                  fontSize,
                  automaticLayout: true,
                }}
                theme={monacoTheme}
              />
            )}
            {activeTab === "preview" && (
              <iframe
                srcDoc={previewSrcDoc}
                sandbox=""
                style={{
                  width: "100%",
                  height: "100%",
                  border: "none",
                }}
                title="Preview"
              />
            )}
            {activeTab === "ast" && (
              <Editor
                height="100%"
                defaultLanguage="json"
                defaultValue={`Click "Generate AST" button to display AST`}
                value={astResult}
                options={{
                  readOnly: true,
                  domReadOnly: true,
                  minimap: { enabled: false },
                  scrollBeyondLastLine: false,
                  fontSize,
                  automaticLayout: true,
                }}
                theme={monacoTheme}
              />
            )}
          </div>
        </div>
      </div>

      {!isEmbed && isProblemsVisible && (
        <>
          <ResizeHandle
            direction="vertical"
            onResize={handleProblemsPanelResize}
          />
          <ProblemsPanel
            problems={diagnostics.items}
            height={problemsPanelHeight}
            onProblemClick={(lineNumber, column) => {
              editorRef.current?.revealLineInCenter(lineNumber);
              editorRef.current?.setPosition({ lineNumber, column });
              editorRef.current?.focus();
            }}
          />
        </>
      )}

      {!isEmbed && (
        <footer className="playground-footer">
          <div className="footer-left">
            <div
              ref={vimStatusBarRef}
              className="vim-status-bar"
              style={{
                display: vimModeEnabled ? "block" : "none",
                minWidth: "100px",
                fontFamily: "monospace",
                fontSize: "11px",
              }}
            />
            {currentFilePath && (
              <>
                <div className="footer-item">
                  <span className="current-file-path">{currentFilePath}</span>
                </div>
                <div className="save-status">
                  {saveStatus === "saved" && (
                    <span className="save-status-item saved">
                      <VscCheck size={14} /> Saved
                    </span>
                  )}
                  {saveStatus === "saving" && (
                    <span className="save-status-item saving">
                      <VscLoading size={14} className="spinning" /> Saving...
                    </span>
                  )}
                  {saveStatus === "unsaved" && (
                    <span className="save-status-item unsaved">
                      <VscSave size={14} /> Unsaved
                    </span>
                  )}
                </div>
              </>
            )}
            {!currentFilePath && isOPFSSupported && (
              <span
                style={{ color: "var(--tree-empty-color)", fontSize: "11px" }}
              >
                No file selected
              </span>
            )}
          </div>
          <div className="footer-right">
            {(diagnostics.errors > 0 || diagnostics.warnings > 0) && (
              <button
                className={`footer-diagnostic-button${isProblemsVisible ? " active" : ""}`}
                onClick={() => setIsProblemsVisible((v) => !v)}
                title={isProblemsVisible ? "Hide Problems" : "Show Problems"}
              >
                {diagnostics.errors > 0 && (
                  <span className="footer-diagnostic footer-diagnostic-error">
                    <VscError size={12} />
                    {diagnostics.errors}
                  </span>
                )}
                {diagnostics.warnings > 0 && (
                  <span className="footer-diagnostic footer-diagnostic-warning">
                    <VscWarning size={12} />
                    {diagnostics.warnings}
                  </span>
                )}
              </button>
            )}
            <span className="cursor-position">
              Ln {cursorPosition.line}, Col {cursorPosition.column}
            </span>
            <button
              className="footer-icon-button"
              onClick={toggleWordWrap}
              title={
                wordWrap === "on" ? "Disable Word Wrap" : "Enable Word Wrap"
              }
              style={{ opacity: wordWrap === "on" ? 1 : 0.5 }}
            >
              <VscWordWrap size={14} />
            </button>
            <button
              className="footer-icon-button"
              onClick={toggleMinimap}
              title={minimapEnabled ? "Disable Minimap" : "Enable Minimap"}
              style={{ opacity: minimapEnabled ? 1 : 0.5 }}
            >
              <VscMap size={14} />
            </button>
            {executionTime && (
              <div className="execution-time">
                {executionTime.toFixed(2)} ms
              </div>
            )}
          </div>
        </footer>
      )}

      {deleteConfirmDialog && (
        <ConfirmDialog
          title="Delete File"
          message={`Are you sure you want to delete "${deleteConfirmDialog.path}"?`}
          confirmLabel="Delete"
          cancelLabel="Cancel"
          onConfirm={confirmDelete}
          onCancel={() => setDeleteConfirmDialog(null)}
        />
      )}

      {tabCloseConfirm && (
        <ConfirmDialog
          title="Unsaved Changes"
          message={`"${tabCloseConfirm.filePath}" has unsaved changes. Close anyway?`}
          confirmLabel="Close"
          cancelLabel="Cancel"
          onConfirm={() => {
            closeTab(tabCloseConfirm.tabId);
            setTabCloseConfirm(null);
          }}
          onCancel={() => setTabCloseConfirm(null)}
        />
      )}

      <ExamplesModal
        isOpen={isExamplesOpen}
        onClose={() => setIsExamplesOpen(false)}
        onSelect={handleChangeExample}
      />

      <SettingsDialog
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
        vimModeEnabled={vimModeEnabled}
        onVimModeToggle={setVimModeEnabled}
        fontSize={fontSize}
        onFontSizeChange={setFontSize}
        theme={theme}
        onThemeChange={setTheme}
        minimapEnabled={minimapEnabled}
        onMinimapToggle={setMinimapEnabled}
        wordWrap={wordWrap}
        onWordWrapToggle={setWordWrap}
        lineNumbers={lineNumbers}
        onLineNumbersToggle={setLineNumbers}
        tabSize={tabSize}
        onTabSizeChange={setTabSize}
        shareMode={shareMode}
        onShareModeChange={setShareMode}
        isOPFSSupported={isOPFSSupported}
      />

      <ToastContainer toasts={toasts} onClose={dismissToast} />
    </div>
  );
};
