import path from "node:path";
import * as vscode from "vscode";
import which from "which";

export class MqDebugConfigurationProvider
  implements vscode.DebugConfigurationProvider
{
  private context: vscode.ExtensionContext;

  constructor(context: vscode.ExtensionContext) {
    this.context = context;
  }

  resolveDebugConfiguration(
    folder: vscode.WorkspaceFolder | undefined,
    config: vscode.DebugConfiguration,
    _token?: vscode.CancellationToken
  ): vscode.ProviderResult<vscode.DebugConfiguration> {
    const editor = vscode.window.activeTextEditor;
    if (editor && editor.document.languageId === "mq") {
      config.type = "mq";
      config.name = "Debug Current File";
      config.request = "launch";
      config.queryFile = "${file}";
      config.stopOnEntry = true;
      config.args = ["dap"];
    }

    if (!config.queryFile) {
      return vscode.window
        .showInformationMessage("Cannot find a query to debug")
        .then((_) => {
          return undefined; // abort launch
        });
    }

    return this.ensureMqDapAvailable().then((mqDapPath) => {
      if (!mqDapPath) {
        return undefined;
      }
      config.runtime = process.env._MQ_DBG_DEBUG_BIN ?? mqDapPath;

      return config;
    });
  }

  private async ensureMqDapAvailable(): Promise<string | undefined> {
    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (workspaceFolder) {
      const localMqDapPath = path.join(
        workspaceFolder.uri.fsPath,
        "target",
        "debug",
        "mq-dbg"
      );

      try {
        await vscode.workspace.fs.stat(vscode.Uri.file(localMqDapPath));
        return localMqDapPath;
      } catch {
        // File doesn't exist, continue to other options
      }
    }

    // Try to find mq-dap in PATH
    const mqDbgPath = await which("mq-dbg", { nothrow: true });
    if (mqDbgPath) {
      return mqDbgPath;
    }

    vscode.window.showErrorMessage(
      'mq-dbg not found in PATH or workspace. Please install it using: cargo install --git https://github.com/harehare/mq.git mq-cli --bin mq-dbg --features="debugger"'
    );
    return undefined;
  }
}
