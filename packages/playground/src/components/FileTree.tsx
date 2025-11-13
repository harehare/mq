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

type NodeType = "file" | "folder";
type FileTreeProps = {
  files: FileNode[];
  onFileSelect: (path: string) => void;
  onRefresh: () => void;
  onCreateFile: (parentPath: string | undefined, fileName: string) => void;
  onCreateFolder: (parentPath: string | undefined, folderName: string) => void;
  onDeleteFile: (path: string) => void;
  onRenameFile: (oldPath: string, newName: string) => void;
  onMoveFile: (sourcePath: string, targetPath: string) => void;
  selectedFile: string | null;
};

type FileTreeNodeProps = {
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
  creatingType: NodeType | null;
  creatingValue: string;
  onCreatingChange: (value: string) => void;
  onCreatingComplete: () => void;
  onCreatingCancel: () => void;
  selectedFile: string | null;
  level: number;
  // Drag and drop
  onDragStart: (node: FileNode) => void;
  onDragEnd: () => void;
  onDragOver: (e: React.DragEvent, node: FileNode) => void;
  onDrop: (e: React.DragEvent, node: FileNode) => void;
  onDragLeave: () => void;
  draggingPath: string | null;
  dragOverPath: string | null;
};

type CreateInputProps = {
  value: string;
  onChange: (value: string) => void;
  onComplete: () => void;
  onCancel: () => void;
  type: NodeType;
  level: number;
};

type DragState = {
  draggingNode: FileNode | null;
  dragOverNode: FileNode | null;
  dragOverRoot: boolean;
};

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
    inputRef.current?.focus();
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
          onBlur={onComplete}
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
  onDragStart,
  onDragEnd,
  onDragOver,
  onDrop,
  onDragLeave,
  draggingPath,
  dragOverPath,
}: FileTreeNodeProps) => {
  const [isExpanded, setIsExpanded] = useState(true);
  const inputRef = useRef<HTMLInputElement>(null);

  const isRenaming = renamingPath === node.path;
  const isDragging = draggingPath === node.path;
  const isDragOver = dragOverPath === node.path;
  const isSelected = selectedFile === node.path;

  // Focus and select input when renaming starts
  useEffect(() => {
    if (isRenaming) {
      inputRef.current?.focus();
      inputRef.current?.select();
    }
  }, [isRenaming]);

  // Event handlers
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
      if (e.key === "Enter") {
        e.preventDefault();
        onRenamingComplete();
      } else if (e.key === "Escape") {
        e.preventDefault();
        onRenamingCancel();
      } else if (e.key === "F2" && !isRenaming) {
        e.preventDefault();
        onStartRename(node);
      }
    },
    [node, isRenaming, onStartRename, onRenamingComplete, onRenamingCancel]
  );

  // Drag and drop handlers
  const handleDragStart = useCallback(
    (e: React.DragEvent) => {
      if (isRenaming) {
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      e.stopPropagation();
      onDragStart(node);
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", node.path);
      e.dataTransfer.setDragImage(e.currentTarget as HTMLElement, 0, 0);
    },
    [node, onDragStart, isRenaming]
  );

  const handleDragEnd = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      onDragEnd();
    },
    [onDragEnd]
  );

  const handleDragOver = useCallback(
    (e: React.DragEvent) => {
      if (node.type === "directory") {
        e.preventDefault();
        e.stopPropagation();
        e.dataTransfer.dropEffect = "move";
        onDragOver(e, node);
      }
    },
    [node, onDragOver]
  );

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      onDrop(e, node);
    },
    [node, onDrop]
  );

  const handleDragLeave = useCallback(
    (e: React.DragEvent) => {
      if (e.currentTarget === e.target) {
        onDragLeave();
      }
    },
    [onDragLeave]
  );

  return (
    <div className="file-tree-node">
      <div
        className={`file-tree-item ${isSelected ? "selected" : ""} ${
          isDragging ? "dragging" : ""
        } ${isDragOver ? "drag-over" : ""}`}
        onClick={handleClick}
        onDoubleClick={handleDoubleClick}
        onContextMenu={handleContextMenu}
        onKeyDown={handleKeyDown}
        onDragStart={handleDragStart}
        onDragEnd={handleDragEnd}
        onDragOver={handleDragOver}
        onDrop={handleDrop}
        onDragLeave={handleDragLeave}
        draggable={!isRenaming}
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
              onDragStart={onDragStart}
              onDragEnd={onDragEnd}
              onDragOver={onDragOver}
              onDrop={onDrop}
              onDragLeave={onDragLeave}
              draggingPath={draggingPath}
              dragOverPath={dragOverPath}
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
  onMoveFile,
  selectedFile,
}: FileTreeProps) => {
  // Context menu state
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    node: FileNode;
  }>();

  // Rename state
  const [renamingNode, setRenamingNode] = useState<FileNode>();
  const [renamingValue, setRenamingValue] = useState("");

  // Create state
  const [creatingItem, setCreatingItem] = useState<{
    parentPath: string | undefined;
    type: NodeType;
  }>();
  const [creatingValue, setCreatingValue] = useState("");

  // Drag and drop state
  const [dragState, setDragState] = useState<DragState>({
    draggingNode: null,
    dragOverNode: null,
    dragOverRoot: false,
  });

  // Context Menu Handlers
  const handleContextMenu = useCallback(
    (e: React.MouseEvent, node: FileNode) => {
      e.preventDefault();
      setContextMenu({ x: e.clientX, y: e.clientY, node });
    },
    []
  );

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
    setRenamingNode(undefined);
    setRenamingValue("");
  }, [renamingNode, renamingValue, onRenameFile]);

  const handleRenamingCancel = useCallback(() => {
    setRenamingNode(undefined);
    setRenamingValue("");
  }, []);

  const handleStartCreate = useCallback(
    (parentPath: string | undefined, type: NodeType) => {
      setCreatingItem({ parentPath, type });
      setCreatingValue("");
    },
    []
  );

  const handleCreatingComplete = useCallback(() => {
    if (creatingItem && creatingValue.trim()) {
      const trimmedValue = creatingValue.trim();

      if (creatingItem.type === "file") {
        onCreateFile(creatingItem.parentPath, trimmedValue);
      } else {
        onCreateFolder(creatingItem.parentPath, trimmedValue);
      }
    }
    setCreatingItem(undefined);
    setCreatingValue("");
  }, [creatingItem, creatingValue, onCreateFile, onCreateFolder]);

  const handleCreatingCancel = useCallback(() => {
    setCreatingItem(undefined);
    setCreatingValue("");
  }, []);

  const handleDragStart = useCallback((node: FileNode) => {
    setDragState((prev) => ({ ...prev, draggingNode: node }));
  }, []);

  const handleDragEnd = useCallback(() => {
    setDragState({
      draggingNode: null,
      dragOverNode: null,
      dragOverRoot: false,
    });
  }, []);

  const handleDragOver = useCallback(
    (_e: React.DragEvent, node: FileNode) => {
      if (
        !dragState.draggingNode ||
        dragState.draggingNode.path === node.path
      ) {
        return;
      }

      // Check if target is a descendant of the dragging node
      const isDescendant = node.path.startsWith(
        dragState.draggingNode.path + "/"
      );
      if (!isDescendant) {
        setDragState((prev) => ({ ...prev, dragOverNode: node }));
      }
    },
    [dragState.draggingNode]
  );

  const handleDrop = useCallback(
    (_e: React.DragEvent, targetNode: FileNode) => {
      if (!dragState.draggingNode || targetNode.type !== "directory") {
        setDragState({
          draggingNode: null,
          dragOverNode: null,
          dragOverRoot: false,
        });
        return;
      }

      // Don't allow dropping a node onto itself or its descendants
      const canDrop =
        dragState.draggingNode.path !== targetNode.path &&
        !targetNode.path.startsWith(dragState.draggingNode.path + "/");

      if (canDrop) {
        onMoveFile(dragState.draggingNode.path, targetNode.path);
      }

      setDragState({
        draggingNode: null,
        dragOverNode: null,
        dragOverRoot: false,
      });
    },
    [dragState.draggingNode, onMoveFile]
  );

  const handleDragLeave = useCallback(() => {
    setDragState((prev) => ({ ...prev, dragOverNode: null }));
  }, []);

  const handleRootDragOver = useCallback(
    (e: React.DragEvent) => {
      if (dragState.draggingNode && e.target === e.currentTarget) {
        e.preventDefault();
        e.stopPropagation();
        e.dataTransfer.dropEffect = "move";
        setDragState((prev) => ({
          ...prev,
          dragOverRoot: true,
          dragOverNode: null,
        }));
      }
    },
    [dragState.draggingNode]
  );

  const handleRootDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();

      if (e.target === e.currentTarget && dragState.draggingNode) {
        const sourceParts = dragState.draggingNode.path
          .split("/")
          .filter(Boolean);
        const fileName = sourceParts[sourceParts.length - 1];
        const newPath = `/${fileName}`;

        // Don't move if already at root
        if (dragState.draggingNode.path !== newPath) {
          onMoveFile(dragState.draggingNode.path, "");
        }
      }

      setDragState({
        draggingNode: null,
        dragOverNode: null,
        dragOverRoot: false,
      });
    },
    [dragState.draggingNode, onMoveFile]
  );

  const handleRootDragLeave = useCallback((e: React.DragEvent) => {
    if (e.target === e.currentTarget) {
      setDragState((prev) => ({ ...prev, dragOverRoot: false }));
    }
  }, []);

  const contentClassName = `file-tree-content ${
    dragState.dragOverRoot ? "drag-over-root" : ""
  }`;

  return (
    <div className="file-tree-container">
      <div className="file-tree-header">
        <div className="file-tree-header-left">
          <span className="file-tree-title">FILES</span>
        </div>
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

      <div
        className={contentClassName}
        onDragOver={handleRootDragOver}
        onDrop={handleRootDrop}
        onDragLeave={handleRootDragLeave}
      >
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
              onDragStart={handleDragStart}
              onDragEnd={handleDragEnd}
              onDragOver={handleDragOver}
              onDrop={handleDrop}
              onDragLeave={handleDragLeave}
              draggingPath={dragState.draggingNode?.path ?? null}
              dragOverPath={dragState.dragOverNode?.path ?? null}
            />
          ))
        )}
      </div>

      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          items={getContextMenuItems()}
          onClose={() => setContextMenu(undefined)}
        />
      )}
    </div>
  );
};
