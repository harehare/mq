import { describe, expect, it, vi } from "vitest";
import { formatDiagnostics } from "../entrypoints/popup/lib/diagnostics";
import { downloadText } from "../entrypoints/popup/lib/download";
import { isRunShortcut } from "../entrypoints/popup/lib/shortcuts";

describe("formatDiagnostics", () => {
  it("includes the source position for every diagnostic", () => {
    expect(
      formatDiagnostics([
        {
          startLine: 2,
          startColumn: 4,
          endLine: 2,
          endColumn: 5,
          message: "expected expression",
        },
      ]),
    ).toBe("Line 2, column 4: expected expression");
  });
});

describe("isRunShortcut", () => {
  it.each([
    [{ key: "Enter", metaKey: true, ctrlKey: false }, true],
    [{ key: "Enter", metaKey: false, ctrlKey: true }, true],
    [{ key: "Enter", metaKey: false, ctrlKey: false }, false],
    [{ key: "a", metaKey: true, ctrlKey: false }, false],
  ])("recognizes %o as %s", (event, expected) => {
    expect(isRunShortcut(event)).toBe(expected);
  });
});

describe("downloadText", () => {
  it("downloads the Markdown and releases the temporary object URL", () => {
    const createObjectUrl = vi.fn(() => "blob:result");
    const revokeObjectUrl = vi.fn();
    const click = vi
      .spyOn(HTMLAnchorElement.prototype, "click")
      .mockImplementation(() => undefined);
    Object.assign(URL, { createObjectURL: createObjectUrl, revokeObjectURL: revokeObjectUrl });

    downloadText("result.md", "# Result");

    expect(createObjectUrl).toHaveBeenCalledOnce();
    expect(click).toHaveBeenCalledOnce();
    expect(revokeObjectUrl).toHaveBeenCalledWith("blob:result");
  });
});
