import type { Heading } from './types';
import { TREE_VIEW_SETTINGS } from './constants';

export const generateTreeView = (markdown: string): Heading[] => {
  const lines = markdown.split('\n');
  const headings: Heading[] = [];
  
  lines.forEach((line, index) => {
    const match = line.match(TREE_VIEW_SETTINGS.HEADING_REGEX);
    if (match) {
      headings.push({
        level: match[1].length,
        text: match[2],
        line: index + 1
      });
    }
  });
  
  return headings;
};

export const clampValue = (value: number, min: number, max: number): number => {
  return Math.min(Math.max(value, min), max);
};

export const getStoredValue = <T>(key: string, defaultValue: T): T => {
  try {
    const stored = localStorage.getItem(key);
    return stored ? JSON.parse(stored) : defaultValue;
  } catch {
    return defaultValue;
  }
};

export const setStoredValue = <T>(key: string, value: T): void => {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch (error) {
    console.warn('Failed to save to localStorage:', error);
  }
};