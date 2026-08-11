import { browser } from "wxt/browser";
import { extractPageHtml } from "../../../lib/injected";

export const PERMISSION_MESSAGE =
  "Couldn't access this tab. Some pages (chrome://, the Chrome Web Store, etc.) can't be scripted by extensions.";

export type ActiveTabResult<T> =
  | { ok: true; value: T }
  | { ok: false; message: string };

async function getActiveTabId(): Promise<number | undefined> {
  const [tab] = await browser.tabs.query({
    active: true,
    currentWindow: true,
  });
  return tab?.id;
}

async function executeInActiveTab<Args extends unknown[], R>(
  func: (...args: Args) => R,
  args: Args,
): Promise<ActiveTabResult<R>> {
  const tabId = await getActiveTabId();
  if (tabId === undefined) {
    return { ok: false, message: "No active tab found." };
  }

  try {
    const [injection] = await browser.scripting.executeScript({
      target: { tabId },
      func,
      args,
    } as Parameters<typeof browser.scripting.executeScript>[0]);
    if (!injection) {
      return { ok: false, message: "The page returned no result." };
    }
    return { ok: true, value: injection.result as R };
  } catch (error) {
    // Fails on tabs scripting can't reach at all (chrome://, the Web
    // Store, etc.), regardless of the activeTab grant.
    const detail = error instanceof Error ? error.message : String(error);
    return { ok: false, message: `${PERMISSION_MESSAGE} (${detail})` };
  }
}

/** Extracts the active tab's full page HTML. */
export function extractActivePageHtml(): Promise<ActiveTabResult<string>> {
  return executeInActiveTab(extractPageHtml, []);
}
