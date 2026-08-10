import { storage } from "wxt/utils/storage";

/**
 * Persists the last entered query so it survives the side panel being
 * closed and reopened (the panel's React tree is torn down on close).
 */
export const queryStorage = storage.defineItem<string>("local:query", {
  fallback: "",
});
