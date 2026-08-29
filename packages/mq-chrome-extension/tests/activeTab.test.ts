import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  executeScript: vi.fn(),
  query: vi.fn(),
}));

vi.mock("wxt/browser", () => ({
  browser: {
    scripting: { executeScript: mocks.executeScript },
    tabs: { query: mocks.query },
  },
}));

import {
  extractActivePageHtml,
  PERMISSION_MESSAGE,
} from "../entrypoints/popup/lib/activeTab";

describe("extractActivePageHtml", () => {
  beforeEach(() => {
    mocks.query.mockReset();
    mocks.executeScript.mockReset();
  });

  it("returns an error when no active tab exists", async () => {
    mocks.query.mockResolvedValue([]);

    await expect(extractActivePageHtml()).resolves.toEqual({
      ok: false,
      message: "No active tab found.",
    });
  });

  it("returns the HTML produced by the injected function", async () => {
    mocks.query.mockResolvedValue([{ id: 42 }]);
    mocks.executeScript.mockResolvedValue([{ result: "<main>Article</main>" }]);

    await expect(extractActivePageHtml()).resolves.toEqual({
      ok: true,
      value: "<main>Article</main>",
    });
    expect(mocks.executeScript).toHaveBeenCalledWith(
      expect.objectContaining({ target: { tabId: 42 }, args: [] }),
    );
  });

  it("explains restricted-page failures", async () => {
    mocks.query.mockResolvedValue([{ id: 42 }]);
    mocks.executeScript.mockRejectedValue(
      new Error("Cannot access a chrome:// URL"),
    );

    await expect(extractActivePageHtml()).resolves.toEqual({
      ok: false,
      message: `${PERMISSION_MESSAGE} (Cannot access a chrome:// URL)`,
    });
  });
});
