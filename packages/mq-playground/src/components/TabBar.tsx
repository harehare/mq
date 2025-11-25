import { VscClose, VscCircleFilled } from "react-icons/vsc";
import { FileIcon } from "./FileIcon";

export type Tab = {
  id: string;
  filePath: string;
  content: string;
  savedContent: string; // Content as last saved to file
  fileType: "mq" | "md" | "mdx" | "other";
  isDirty: boolean;
};

type TabBarProps = {
  tabs: Tab[];
  activeTabId: string | null;
  onTabClick: (tabId: string) => void;
  onTabClose: (tabId: string) => void;
};

export const TabBar = ({
  tabs,
  activeTabId,
  onTabClick,
  onTabClose,
}: TabBarProps) => {
  const getFileName = (filePath: string) => {
    const parts = filePath.split("/");
    return parts[parts.length - 1] || filePath;
  };

  const handleTabClose = (e: React.MouseEvent, tabId: string) => {
    e.stopPropagation();
    onTabClose(tabId);
  };

  return (
    <div className="tab-bar">
      {tabs.map((tab) => (
        <div
          key={tab.id}
          className={`tab-item ${activeTabId === tab.id ? "active" : ""}`}
          onClick={() => onTabClick(tab.id)}
          title={tab.filePath}
        >
          <div className="tab-content">
            <FileIcon fileName={getFileName(tab.filePath)} isDirectory={false} />
            <span className="tab-label">{getFileName(tab.filePath)}</span>
            {tab.isDirty && (
              <VscCircleFilled className="tab-dirty-indicator" size={8} />
            )}
          </div>
          <button
            className="tab-close-button"
            onClick={(e) => handleTabClose(e, tab.id)}
            title="Close"
          >
            <VscClose size={14} />
          </button>
        </div>
      ))}
    </div>
  );
};
