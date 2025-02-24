import * as lc from "vscode-languageclient/node";
import * as vscode from "vscode";
import which from "which";

let client: lc.LanguageClient;

export async function activate(context: vscode.ExtensionContext) {
  vscode.commands.registerCommand("mq-lsp.installLSP", async () => {
    await installLspServer();
  });

  let lspPath: string | null;

  if (process.env._MDQ_DEBUG_BIN) {
    lspPath = process.env._MDQ_DEBUG_BIN;
  } else {
    const config = vscode.workspace.getConfiguration("mq-lsp");
    const configLspPath = config.get<string>("lspPath");

    if (configLspPath) {
      lspPath = configLspPath;
    } else {
      lspPath = await which("mq-lsp", { nothrow: true });

      if (lspPath === null) {
        const selected = await vscode.window.showInformationMessage(
          "Install mq-lsp-server?",
          "Yes",
          "No"
        );

        if (selected === "Yes") {
          await installLspServer();
        }

        return;
      }
    }
  }

  const run: lc.Executable = {
    command: lspPath,
    args: ["language-server"],
    options: {
      cwd: ".",
    },
  };

  const serverOptions: lc.ServerOptions = {
    run,
    debug: {
      ...run,
      options: {
        ...run.options,
      },
    },
  };

  const clientOptions: lc.LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "mq" }],
    initializationOptions: vscode.workspace.getConfiguration("mq"),
  };

  client = new lc.LanguageClient(
    "mq",
    "mq Language Server",
    serverOptions,
    clientOptions
  );

  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

const installLspServer = async () => {
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
    new vscode.ShellExecution("cargo install mq-lsp")
  );

  try {
    const execution = await vscode.tasks.executeTask(task);

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
};
