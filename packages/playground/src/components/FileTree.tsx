import { useState, useCallback, useRef, useEffect } from "react";
import { FileNode } from "../utils/fileSystem";
import { FileIcon } from "./FileIcon";
import {
  VscChevronRight,
  VscChevronDown,
  VscNewFile,
  VscNewFolder,
  VscRefresh,
  VscTrash,
  VscEdit,
} from "react-icons/vsc";
import "./FileTree.css";
import { ContextMenu, ContextMenuItem } from "./ContextMenu";

interface FileTreeProps {
  files: FileNode[];
  onFileSelect: (path: string) => void;
  onRefresh: () => void;
  onCreateFile: (parentPath: string | undefined, fileName: string) => void;
  onCreateFolder: (parentPath: string | undefined, folderName: string) => void;
  onDeleteFile: (path: string) => void;
  onRenameFile: (oldPath: string, newName: string) => void;
  selectedFile: string | null;
}

interface FileTreeNodeProps {
  node: FileNode;
  onFileSelect: (path: string) => void;
  onContextMenu: (e: React.MouseEvent, node: FileNode) => void;
  onStartRename: (node: FileNode) => void;
  renamingPath: string | null;
  renamingValue: string;
  onRenamingChange: (value: string) => void;
  onRenamingComplete: () => void;
  onRenamingCancel: () => void;
  creatingInPath: string | undefined;
  creatingType: "file" | "folder" | null;
  creatingValue: string;
  onCreatingChange: (value: string) => void;
  onCreatingComplete: () => void;
  onCreatingCancel: () => void;
  selectedFile: string | null;
  level: number;
}

interface CreateInputProps {
  value: string;
  onChange: (value: string) => void;
  onComplete: () => void;
  onCancel: () => void;
  type: "file" | "folder";
  level: number;
}

const CreateInput = ({
  value,
  onChange,
  onComplete,
  onCancel,
  type,
  level,
}: CreateInputProps) => {
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (inputRef.current) {
      inputRef.current.focus();
    }
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      onComplete();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onCancel();
    }
  };

  const handleBlur = () => {
    // Delay the onComplete call to ensure onChange has updated the state
    setTimeout(() => {
      onComplete();
    }, 100);
  };

  return (
    <div className="file-tree-node">
      <div
        className="file-tree-item creating"
        style={{ paddingLeft: `${level * 12 + 4}px` }}
      >
        <span className="file-tree-spacer" />
        <FileIcon
          fileName={type === "folder" ? "" : "file.txt"}
          isDirectory={type === "folder"}
          isExpanded={false}
        />
        <input
          ref={inputRef}
          type="text"
          className="file-tree-rename-input"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onBlur={handleBlur}
          onKeyDown={handleKeyDown}
          placeholder={type === "folder" ? "Folder name" : "File name"}
        />
      </div>
    </div>
  );
};

const FileTreeNode = ({
  node,
  onFileSelect,
  onContextMenu,
  onStartRename,
  renamingPath,
  renamingValue,
  onRenamingChange,
  onRenamingComplete,
  onRenamingCancel,
  creatingInPath,
  creatingType,
  creatingValue,
  onCreatingChange,
  onCreatingComplete,
  onCreatingCancel,
  selectedFile,
  level,
}: FileTreeNodeProps) => {
  const [isExpanded, setIsExpanded] = useState(true);
  const inputRef = useRef<HTMLInputElement>(null);
  const isRenaming = renamingPath === node.path;

  useEffect(() => {
    if (isRenaming && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [isRenaming]);

  const handleClick = useCallback(() => {
    if (node.type === "directory") {
      setIsExpanded(!isExpanded);
    } else {
      onFileSelect(node.path);
    }
  }, [node, isExpanded, onFileSelect]);

  const handleDoubleClick = useCallback(() => {
    onStartRename(node);
  }, [node, onStartRename]);

  const handleContextMenu = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      onContextMenu(e, node);
    },
    [node, onContextMenu]
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        onRenamingComplete();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        onRenamingCancel();
      } else if (e.key === 'F2' && !isRenaming) {
        e.preventDefault();
        onStartRename(node);
      }
    },
    [node, isRenaming, onStartRename, onRenamingComplete, onRenamingCancel]
  );

  const isSelected = selectedFile === node.path;

  return (
    <div className="file-tree-node">
      <div
        className={`file-tree-item ${isSelected ? "selected" : ""}`}
        onClick={handleClick}
        onDoubleClick={handleDoubleClick}
        onContextMenu={handleContextMenu}
        onKeyDown={handleKeyDown}
        tabIndex={0}
        style={{ paddingLeft: `${level * 12 + 4}px` }}
      >
        {node.type === "directory" && (
          <span className="file-tree-chevron">
            {isExpanded ? (
              <VscChevronDown size={16} />
            ) : (
              <VscChevronRight size={16} />
            )}
          </span>
        )}
        {node.type === "file" && <span className="file-tree-spacer" />}
        <FileIcon
          fileName={node.name}
          isDirectory={node.type === "directory"}
          isExpanded={isExpanded}
        />
        {isRenaming ? (
          <input
            ref={inputRef}
            type="text"
            className="file-tree-rename-input"
            value={renamingValue}
            onChange={(e) => onRenamingChange(e.target.value)}
            onBlur={onRenamingComplete}
            onKeyDown={handleKeyDown}
            onClick={(e) => e.stopPropagation()}
          />
        ) : (
          <span className="file-tree-name">{node.name}</span>
        )}
      </div>
      {node.type === "directory" && isExpanded && (
        <div className="file-tree-children">
          {creatingInPath === node.path && creatingType && (
            <CreateInput
              value={creatingValue}
              onChange={onCreatingChange}
              onComplete={onCreatingComplete}
              onCancel={onCreatingCancel}
              type={creatingType}
              level={level + 1}
            />
          )}
          {node.children?.map((child) => (
            <FileTreeNode
              key={child.path}
              node={child}
              onFileSelect={onFileSelect}
              onContextMenu={onContextMenu}
              onStartRename={onStartRename}
              renamingPath={renamingPath}
              renamingValue={renamingValue}
              onRenamingChange={onRenamingChange}
              onRenamingComplete={onRenamingComplete}
              onRenamingCancel={onRenamingCancel}
              creatingInPath={creatingInPath}
              creatingType={creatingType}
              creatingValue={creatingValue}
              onCreatingChange={onCreatingChange}
              onCreatingComplete={onCreatingComplete}
              onCreatingCancel={onCreatingCancel}
              selectedFile={selectedFile}
              level={level + 1}
            />
          ))}
        </div>
      )}
    </div>
  );
};

export const FileTree = ({
  files,
  onFileSelect,
  onRefresh,
  onCreateFile,
  onCreateFolder,
  onDeleteFile,
  onRenameFile,
  selectedFile,
}: FileTreeProps) => {
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    node: FileNode;
  } | null>(null);
  const [renamingNode, setRenamingNode] = useState<FileNode | null>(null);
  const [renamingValue, setRenamingValue] = useState("");
  const [creatingItem, setCreatingItem] = useState<{
    parentPath: string | undefined;
    type: "file" | "folder";
  } | null>(null);
  const [creatingValue, setCreatingValue] = useState("");

  const handleContextMenu = useCallback(
    (e: React.MouseEvent, node: FileNode) => {
      e.preventDefault();
      setContextMenu({ x: e.clientX, y: e.clientY, node });
    },
    []
  );

  const handleStartRename = useCallback((node: FileNode) => {
    setRenamingNode(node);
    setRenamingValue(node.name);
  }, []);

  const handleRenamingComplete = useCallback(() => {
    if (renamingNode) {
      const trimmedValue = renamingValue.trim();
      if (trimmedValue && trimmedValue !== renamingNode.name) {
        onRenameFile(renamingNode.path, trimmedValue);
      }
    }
    setRenamingNode(null);
    setRenamingValue("");
  }, [renamingNode, renamingValue, onRenameFile]);

  const handleRenamingCancel = useCallback(() => {
    setRenamingNode(null);
    setRenamingValue("");
  }, []);

  const handleStartCreate = useCallback((parentPath: string | undefined, type: "file" | "folder") => {
    setCreatingItem({ parentPath, type });
    setCreatingValue("");
  }, []);

  const handleCreatingComplete = useCallback(() => {
    console.log("handleCreatingComplete called:", { creatingItem, creatingValue });

    if (creatingItem && creatingValue.trim()) {
      const trimmedValue = creatingValue.trim();
      console.log("Creating:", { type: creatingItem.type, parentPath: creatingItem.parentPath, value: trimmedValue });

      if (creatingItem.type === "file") {
        onCreateFile(creatingItem.parentPath, trimmedValue);
      } else {
        onCreateFolder(creatingItem.parentPath, trimmedValue);
      }
    } else {
      console.log("Skipping creation - empty value or no creatingItem");
    }
    setCreatingItem(null);
    setCreatingValue("");
  }, [creatingItem, creatingValue, onCreateFile, onCreateFolder]);

  const handleCreatingCancel = useCallback(() => {
    setCreatingItem(null);
    setCreatingValue("");
  }, []);

  const getContextMenuItems = (): ContextMenuItem[] => {
    if (!contextMenu) return [];

    const { node } = contextMenu;
    const items: ContextMenuItem[] = [];

    if (node.type === "directory") {
      items.push({
        label: "New File",
        icon: <VscNewFile size={16} />,
        onClick: () => handleStartCreate(node.path, "file"),
      });
      items.push({
        label: "New Folder",
        icon: <VscNewFolder size={16} />,
        onClick: () => handleStartCreate(node.path, "folder"),
      });
    }

    items.push({
      label: "Rename",
      icon: <VscEdit size={16} />,
      onClick: () => handleStartRename(node),
    });

    items.push({
      label: "Delete",
      icon: <VscTrash size={16} />,
      onClick: () => onDeleteFile(node.path),
    });

    return items;
  };

  return (
    <div className="file-tree-container">
      <div className="file-tree-header">
        <span className="file-tree-title">FILES</span>
        <div className="file-tree-actions">
          <button
            className="file-tree-action-btn"
            onClick={() => handleStartCreate(undefined, "file")}
            title="New File"
          >
            <VscNewFile size={16} />
          </button>
          <button
            className="file-tree-action-btn"
            onClick={() => handleStartCreate(undefined, "folder")}
            title="New Folder"
          >
            <VscNewFolder size={16} />
          </button>
          <button
            className="file-tree-action-btn"
            onClick={onRefresh}
            title="Refresh"
          >
            <VscRefresh size={16} />
          </button>
        </div>
      </div>
      <div className="file-tree-content">
        {creatingItem?.parentPath === undefined && creatingItem?.type && (
          <CreateInput
            value={creatingValue}
            onChange={setCreatingValue}
            onComplete={handleCreatingComplete}
            onCancel={handleCreatingCancel}
            type={creatingItem.type}
            level={0}
          />
        )}
        {files.length === 0 && !creatingItem ? (
          <div className="file-tree-empty">No files</div>
        ) : (
          files.map((node) => (
            <FileTreeNode
              key={node.path}
              node={node}
              onFileSelect={onFileSelect}
              onContextMenu={handleContextMenu}
              onStartRename={handleStartRename}
              renamingPath={renamingNode?.path ?? null}
              renamingValue={renamingValue}
              onRenamingChange={setRenamingValue}
              onRenamingComplete={handleRenamingComplete}
              onRenamingCancel={handleRenamingCancel}
              creatingInPath={creatingItem?.parentPath}
              creatingType={creatingItem?.type ?? null}
              creatingValue={creatingValue}
              onCreatingChange={setCreatingValue}
              onCreatingComplete={handleCreatingComplete}
              onCreatingCancel={handleCreatingCancel}
              selectedFile={selectedFile}
              level={0}
            />
          ))
        )}
      </div>
      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          items={getContextMenuItems()}
          onClose={() => setContextMenu(null)}
        />
      )}
    </div>
  );
};
