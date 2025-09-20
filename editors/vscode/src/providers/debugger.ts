import * as vscode from "vscode";
import which from "which";

const MQ_DAP_VERSION_KEY = "mq.dapVersion" as const;

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
    // If launch.json is missing or empty
    if (!config.type && !config.request && !config.name) {
      const editor = vscode.window.activeTextEditor;
      if (editor && editor.document.languageId === "mq") {
        config.type = "mq";
        config.name = "Debug Current File";
        config.request = "launch";
        config.queryFile = "${file}";
        config.stopOnEntry = true;
      }
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
      config.runtime = mqDapPath;

      return config;
    });
  }

  private async ensureMqDapAvailable(): Promise<string | undefined> {
    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (workspaceFolder) {
      // const localMqDapPath = path.join(
      //   workspaceFolder.uri.fsPath,
      //   "target",
      //   "debug",
      //   "mq-dap"
      // );
      const localMqDapPath = "/Users/harehare/git/mq/target/debug/mq-dap";

      try {
        await vscode.workspace.fs.stat(vscode.Uri.file(localMqDapPath));
        return localMqDapPath;
      } catch {
        // File doesn't exist, continue to other options
      }
    }

    // Try to find mq-dap in PATH
    const mqDapPath = await which("mq-dap", { nothrow: true });
    if (mqDapPath) {
      return mqDapPath;
    }

    const prevVersion =
      this.context.globalState.get<string>(MQ_DAP_VERSION_KEY);
    const currentVersion = this.context.extension.packageJSON.version;

    if (!mqDapPath || !prevVersion) {
      const installResult = await vscode.window.showInformationMessage(
        "mq-dap not found. Would you like to install it?",
        "Yes",
        "No"
      );

      if (installResult === "Yes") {
        await this.installDap(this.context);
        return "mq-dap";
      }
    } else if (currentVersion !== prevVersion) {
      const installResult = await vscode.window.showInformationMessage(
        "mq-dap version has changed. Would you like to reinstall it?",
        "Yes",
        "No"
      );

      if (installResult === "Yes") {
        await this.installDap(this.context);
        return "mq-dap";
      }
    }

    vscode.window.showErrorMessage(
      "mq-dap debug adapter not found. Please build the mq project or install mq-dap."
    );
    return undefined;
  }

  private async installDap(context: vscode.ExtensionContext): Promise<boolean> {
    const cargoPath = await which("cargo", { nothrow: true });

    if (cargoPath === null) {
      vscode.window.showErrorMessage("Cargo not found in PATH");
      return false;
    }

    const task = new vscode.Task(
      { type: "cargo", task: "install-lsp" },
      vscode.TaskScope.Workspace,
      "Install LSP Server",
      "mq-lsp",
      new vscode.ShellExecution(
        `cargo install --git https://github.com/harehare/mq.git mq-dap --branch feat/add-dap-debug-adapter`
      )
    );

    try {
      const execution = await vscode.tasks.executeTask(task);
      await context.globalState.update(
        MQ_DAP_VERSION_KEY,
        context.extension.packageJSON.version
      );

      return new Promise<boolean>((resolve) => {
        const disposable = vscode.tasks.onDidEndTaskProcess((e) => {
          if (e.execution === execution) {
            disposable.dispose();
            resolve(e.exitCode === 0);
          }
        });
      });
    } catch (error) {
      vscode.window.showErrorMessage(
        `Installation task failed: ${
          error instanceof Error ? error.message : "Unknown error"
        }`
      );
      return false;
    }
  }
}
