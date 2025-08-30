export const PANEL_WIDTH_CONSTRAINTS = {
  MIN: 20,
  MAX: 80,
  DEFAULT: 50,
} as const;

export const STORAGE_KEYS = {
  DARK_MODE: "darkMode",
} as const;

export const TREE_VIEW_SETTINGS = {
  HEADING_REGEX: /^(#{1,6})\s+(.+)$/,
  INDENT_PX_PER_LEVEL: 16,
} as const;
