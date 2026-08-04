import { browser } from "wxt/browser";
import { extractPageHtml, toggleOverlayPreview } from "../../../lib/injected";

export const PERMISSION_MESSAGE =
  "Couldn't access this tab. Click the mq icon in the toolbar again to grant access, then retry.";

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
  } catch {
    // activeTab's grant is scoped to the tab active when the toolbar icon
    // (or another qualifying gesture) was last clicked, and isn't renewed
    // just by switching tabs while the side panel stays open.
    return { ok: false, message: PERMISSION_MESSAGE };
  }
}

/** Extracts the active tab's full page HTML. */
export function extractActivePageHtml(): Promise<ActiveTabResult<string>> {
  return executeInActiveTab(extractPageHtml, []);
}

/** Toggles the on-page preview overlay in the active tab. Returns whether it's now visible. */
export function toggleActivePagePreview(
  srcdoc: string,
): Promise<ActiveTabResult<boolean>> {
  return executeInActiveTab(toggleOverlayPreview, [srcdoc]);
}
