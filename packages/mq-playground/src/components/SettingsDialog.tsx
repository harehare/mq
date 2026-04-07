import { VscClose } from "react-icons/vsc";
import "./SettingsDialog.css";

type SettingsDialogProps = {
  isOpen: boolean;
  onClose: () => void;
  vimModeEnabled: boolean;
  onVimModeToggle: (enabled: boolean) => void;
  fontSize: number;
  onFontSizeChange: (size: number) => void;
  theme: "light" | "dark" | "system";
  onThemeChange: (theme: "light" | "dark" | "system") => void;
  minimapEnabled: boolean;
  onMinimapToggle: (enabled: boolean) => void;
  wordWrap: "on" | "off";
  onWordWrapToggle: (wordWrap: "on" | "off") => void;
  lineNumbers: "on" | "off";
  onLineNumbersToggle: (lineNumbers: "on" | "off") => void;
  tabSize: number;
  onTabSizeChange: (size: number) => void;
};

export const SettingsDialog = ({
  isOpen,
  onClose,
  vimModeEnabled,
  onVimModeToggle,
  fontSize,
  onFontSizeChange,
  theme,
  onThemeChange,
  minimapEnabled,
  onMinimapToggle,
  wordWrap,
  onWordWrapToggle,
  lineNumbers,
  onLineNumbersToggle,
  tabSize,
  onTabSizeChange,
}: SettingsDialogProps) => {
  if (!isOpen) return null;

  return (
    <div className="settings-overlay" onClick={onClose}>
      <div className="settings-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="settings-header">
          <h3>Settings</h3>
          <button className="settings-close-btn" onClick={onClose}>
            <VscClose size={20} />
          </button>
        </div>
        <div className="settings-content">
          <div className="settings-section">
            <h4>Editor</h4>
            <div className="settings-item">
              <label htmlFor="vim-mode">Vim Mode</label>
              <input
                id="vim-mode"
                type="checkbox"
                checked={vimModeEnabled}
                onChange={(e) => onVimModeToggle(e.target.checked)}
              />
            </div>
            <div className="settings-item">
              <label htmlFor="minimap">Minimap</label>
              <input
                id="minimap"
                type="checkbox"
                checked={minimapEnabled}
                onChange={(e) => onMinimapToggle(e.target.checked)}
              />
            </div>
            <div className="settings-item">
              <label htmlFor="word-wrap">Word Wrap</label>
              <input
                id="word-wrap"
                type="checkbox"
                checked={wordWrap === "on"}
                onChange={(e) =>
                  onWordWrapToggle(e.target.checked ? "on" : "off")
                }
              />
            </div>
            <div className="settings-item">
              <label htmlFor="line-numbers">Line Numbers</label>
              <input
                id="line-numbers"
                type="checkbox"
                checked={lineNumbers === "on"}
                onChange={(e) =>
                  onLineNumbersToggle(e.target.checked ? "on" : "off")
                }
              />
            </div>
            <div className="settings-item">
              <label htmlFor="font-size">Font Size</label>
              <div className="font-size-control">
                <input
                  id="font-size"
                  type="range"
                  min="10"
                  max="24"
                  value={fontSize}
                  onChange={(e) => onFontSizeChange(parseInt(e.target.value))}
                />
                <span>{fontSize}px</span>
              </div>
            </div>
            <div className="settings-item">
              <label htmlFor="tab-size">Tab Size</label>
              <select
                id="tab-size"
                className="settings-select"
                value={tabSize}
                onChange={(e) => onTabSizeChange(parseInt(e.target.value))}
              >
                <option value="2">2</option>
                <option value="4">4</option>
                <option value="8">8</option>
              </select>
            </div>
          </div>
          <div className="settings-section">
            <h4>Appearance</h4>
            <div className="settings-item">
              <label htmlFor="theme-select">Theme</label>
              <select
                id="theme-select"
                className="settings-select"
                value={theme}
                onChange={(e) =>
                  onThemeChange(e.target.value as "light" | "dark" | "system")
                }
              >
                <option value="system">System</option>
                <option value="light">Light</option>
                <option value="dark">Dark</option>
              </select>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
