import * as vscode from "vscode";

export class MqCodeLensProvider implements vscode.CodeLensProvider {
  async provideCodeLenses(
    document: vscode.TextDocument
  ): Promise<vscode.CodeLens[]> {
    const codeLenses: vscode.CodeLens[] = [];
    const lines = document.getText().split("\n");
    let i = 0;

    while (i < lines.length) {
      const line = lines[i].trim();

      // Skip empty lines and comments
      if (!line || line.startsWith("#")) {
        i++;
        continue;
      }

      // Handle queries starting with '.'
      if (line.startsWith(".")) {
        const { endLine, query } = this.collectQueryBlock(lines, i);
        if (query) {
          codeLenses.push(
            new vscode.CodeLens(
              new vscode.Range(
                new vscode.Position(i, 0),
                new vscode.Position(endLine, lines[endLine].length)
              ),
              {
                title: "▶︎ Run Query",
                command: "mq.runQueryAndShowInEditor",
                arguments: [query],
              }
            )
          );
        }
        i = endLine + 1;
        continue;
      }

      // Handle 'def' function blocks or other queries
      const matchInfo = this.matchQueryBlock(lines, i);
      if (matchInfo) {
        const { matchLines, query } = matchInfo;
        codeLenses.push(
          new vscode.CodeLens(
            new vscode.Range(
              new vscode.Position(i, 0),
              new vscode.Position(
                i + matchLines - 1,
                lines[i + matchLines - 1].length
              )
            ),
            {
              title: "▶︎ Run Query",
              command: "mq.runQueryAndShowInEditor",
              arguments: [query],
            }
          )
        );
        i += matchLines;
      } else {
        i++;
      }
    }

    return codeLenses;
  }

  private collectQueryBlock(
    lines: string[],
    startLine: number
  ): { endLine: number; query: string } {
    let endLine = startLine;
    while (endLine + 1 < lines.length && lines[endLine + 1].trim() !== "") {
      endLine++;
    }
    const query = lines
      .slice(startLine, endLine + 1)
      .join("\n")
      .trim();
    return { endLine, query };
  }

  private matchQueryBlock(
    lines: string[],
    startLine: number
  ): { matchLines: number; query: string } | null {
    const queryRegex = /(^def[\s\S]+?;\s*$)/gm;
    const remainingText = lines.slice(startLine).join("\n");
    const match = queryRegex.exec(remainingText);

    if (match && match.index === 0) {
      const matchLines = match[0].split("\n").length;
      const query = match[0].trim();
      return { matchLines, query };
    }
    return null;
  }
}
