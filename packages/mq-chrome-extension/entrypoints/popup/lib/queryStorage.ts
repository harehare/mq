import { storage } from "wxt/utils/storage";

/** Query used when the user has not saved a query yet. */
export const DEFAULT_QUERY = ".";

/**
 * Persists the last entered query so it survives the popup being closed
 * and reopened (the popup's React tree is torn down on close).
 */
export const queryStorage = storage.defineItem<string>("local:query", {
  fallback: DEFAULT_QUERY,
});
