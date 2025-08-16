export interface Tool {
  id: string;
  name: string;
  description: string;
  path: string;
  transform: (input: string) => Promise<string>;
}

export interface Heading {
  level: number;
  text: string;
  line: number;
}

export type ViewMode = "text" | "preview";

export interface AppState {
  inputText: string;
  outputText: string;
  viewMode: ViewMode;
  isDarkMode: boolean;
  isTreeViewOpen: boolean;
  leftPanelWidth: number;
  isResizing: boolean;
}
