import type { Diagnostic } from "./mq";

/** Formats mq diagnostics into a concise message suitable for the popup UI. */
export function formatDiagnostics(diagnostics: ReadonlyArray<Diagnostic>): string {
  return diagnostics
    .map(
      ({ startLine, startColumn, message }) =>
        `Line ${startLine}, column ${startColumn}: ${message}`,
    )
    .join("\n");
}
