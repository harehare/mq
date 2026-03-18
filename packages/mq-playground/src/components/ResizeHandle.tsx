import { useRef, useCallback, useEffect } from "react";
import "./ResizeHandle.css";

type ResizeHandleProps = {
  direction: "horizontal" | "vertical";
  onResize: (delta: number) => void;
};

const KEYBOARD_RESIZE_STEP = 10;

export const ResizeHandle = ({ direction, onResize }: ResizeHandleProps) => {
  const isDragging = useRef(false);
  const lastPos = useRef(0);

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      isDragging.current = true;
      lastPos.current = direction === "horizontal" ? e.clientX : e.clientY;
      document.body.style.cursor =
        direction === "horizontal" ? "col-resize" : "row-resize";
      document.body.style.userSelect = "none";
    },
    [direction],
  );

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      let delta = 0;

      if (direction === "horizontal") {
        if (event.key === "ArrowLeft") {
          delta = -KEYBOARD_RESIZE_STEP;
        } else if (event.key === "ArrowRight") {
          delta = KEYBOARD_RESIZE_STEP;
        }
      } else {
        if (event.key === "ArrowUp") {
          delta = -KEYBOARD_RESIZE_STEP;
        } else if (event.key === "ArrowDown") {
          delta = KEYBOARD_RESIZE_STEP;
        }
      }

      if (delta !== 0) {
        event.preventDefault();
        onResize(delta);
      }
    },
    [direction, onResize],
  );

  const resetBodyStyles = useCallback(() => {
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
  }, []);

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!isDragging.current) return;
      const currentPos = direction === "horizontal" ? e.clientX : e.clientY;
      const delta = currentPos - lastPos.current;
      lastPos.current = currentPos;
      onResize(delta);
    };

    const handleMouseUp = () => {
      if (!isDragging.current) return;
      isDragging.current = false;
      resetBodyStyles();
    };

    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);
    return () => {
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
      if (isDragging.current) {
        isDragging.current = false;
        resetBodyStyles();
      }
    };
  }, [direction, onResize, resetBodyStyles]);

  return (
    <div
      className={`resize-handle resize-handle-${direction}`}
      role="separator"
      aria-orientation={direction}
      tabIndex={0}
      onMouseDown={handleMouseDown}
      onKeyDown={handleKeyDown}
    />
  );
};
