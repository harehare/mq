import { useEffect, useLayoutEffect, useRef, useState } from "react";
import "./ContextMenu.css";

export type ContextMenuItem =
  | { type: "separator" }
  | {
      type?: "item";
      label: string;
      onClick: () => void;
      icon?: React.ReactNode;
      disabled?: boolean;
    };

type ContextMenuProps = {
  x: number;
  y: number;
  items: ContextMenuItem[];
  onClose: () => void;
};

export const ContextMenu = ({ x, y, items, onClose }: ContextMenuProps) => {
  const menuRef = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ x, y });

  useLayoutEffect(() => {
    if (!menuRef.current) return;
    const rect = menuRef.current.getBoundingClientRect();
    const clampedX = Math.min(x, window.innerWidth - rect.width - 8);
    const clampedY = Math.min(y, window.innerHeight - rect.height - 8);
    setPosition({ x: Math.max(0, clampedX), y: Math.max(0, clampedY) });
  }, [x, y]);

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };

    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
      }
    };

    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleEscape);

    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleEscape);
    };
  }, [onClose]);

  return (
    <div
      ref={menuRef}
      className="context-menu"
      style={{ top: `${position.y}px`, left: `${position.x}px` }}
    >
      {items.map((item, index) =>
        item.type === "separator" ? (
          <div key={index} className="context-menu-separator" />
        ) : (
          <div
            key={index}
            className={`context-menu-item ${item.disabled ? "disabled" : ""}`}
            onClick={() => {
              if (!item.disabled) {
                item.onClick();
                onClose();
              }
            }}
          >
            {item.icon && (
              <span className="context-menu-icon">{item.icon}</span>
            )}
            <span className="context-menu-label">{item.label}</span>
          </div>
        ),
      )}
    </div>
  );
};
