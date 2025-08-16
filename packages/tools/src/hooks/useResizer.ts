import { useState, useEffect, RefObject } from 'react';
import { clampValue } from '../utils';
import { PANEL_WIDTH_CONSTRAINTS } from '../constants';

interface UseResizerProps {
  containerRef: RefObject<HTMLDivElement>;
}

export const useResizer = ({ containerRef }: UseResizerProps) => {
  const [leftPanelWidth, setLeftPanelWidth] = useState(PANEL_WIDTH_CONSTRAINTS.DEFAULT);
  const [isResizing, setIsResizing] = useState(false);

  const handleMouseDown = () => {
    setIsResizing(true);
  };

  const handleMouseMove = (e: MouseEvent) => {
    if (!isResizing || !containerRef.current) return;
    
    const containerRect = containerRef.current.getBoundingClientRect();
    const newLeftWidth = ((e.clientX - containerRect.left) / containerRect.width) * 100;
    
    const clampedWidth = clampValue(
      newLeftWidth, 
      PANEL_WIDTH_CONSTRAINTS.MIN, 
      PANEL_WIDTH_CONSTRAINTS.MAX
    );
    setLeftPanelWidth(clampedWidth);
  };

  const handleMouseUp = () => {
    setIsResizing(false);
  };

  useEffect(() => {
    if (isResizing) {
      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
    } else {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    }

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
  }, [isResizing]);

  return {
    leftPanelWidth,
    isResizing,
    handleMouseDown
  };
};