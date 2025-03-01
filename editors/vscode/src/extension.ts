import * as lc from "vscode-languageclient/node";
import * as vscode from "vscode";
import which from "which";

let client: lc.LanguageClient | null = null;

export async function activate(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.commands.registerCommand("mq-lsp.installLSPServer", async () => {
      await installLspServer();
      await startLspServer();
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("mq-lsp.startLSPServer", async () => {
      if (client) {
        await client.stop();
        client = null;
      }

      await startLspServer();
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("mq-lsp.runSelectedText", async () => {
      if (!client) {
        vscode.window.showErrorMessage("LSP server is not running");
        return;
      }

      const editor = vscode.window.activeTextEditor;
      if (!editor) {
        vscode.window.showErrorMessage("No active editor");
        return;
      }

      const selection = editor.selection;
      const selectedText = editor.document.getText(selection);

      if (!selectedText) {
        vscode.window.showErrorMessage("No text selected");
        return;
      }

      try {
        const result = await client.sendRequest(lc.ExecuteCommandRequest.type, {
          command: "mq/runSelectedText",
          arguments: [selectedText],
        });

        if (result) {
          const outputChannel = vscode.window.createOutputChannel(
            "mq LSP Output",
            "markdown"
          );
          outputChannel.clear();
          outputChannel.appendLine(result);
          outputChannel.show();
        }
      } catch (error) {
        vscode.window.showErrorMessage(
          `Failed to run text: ${
            error instanceof Error ? error.message : "Unknown error"
          }`
        );
      }
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      "mq-lsp.setSelectedTextAsInput",
      async () => {
        if (!client) {
          vscode.window.showErrorMessage("LSP server is not running");
          return;
        }

        const editor = vscode.window.activeTextEditor;
        if (!editor) {
          vscode.window.showErrorMessage("No active editor");
          return;
        }

        const selection = editor.selection;
        const selectedText = editor.document.getText(selection);

        if (!selectedText) {
          vscode.window.showErrorMessage("No text selected");
          return;
        }

        try {
          const result = await client.sendRequest(
            lc.ExecuteCommandRequest.type,
            {
              command: "mq/setSelectedTextAsInput",
              arguments: [selectedText],
            }
          );

          if (result) {
            const outputChannel = vscode.window.createOutputChannel(
              "mq LSP Output",
              "markdown"
            );
            outputChannel.clear();
            outputChannel.appendLine(result);
            outputChannel.show();
          }
        } catch (error) {
          vscode.window.showErrorMessage(
            `Failed to run text: ${
              error instanceof Error ? error.message : "Unknown error"
            }`
          );
        }
      }
    )
  );

  if (process.env._MQ_DEBUG_BIN) {
    await startLspServer();
  } else {
    const config = vscode.workspace.getConfiguration("mq-lsp");
    const configLspPath = config.get<string>("lspPath");

    if (configLspPath) {
      await startLspServer();
    } else {
      if ((await which("mq-lsp", { nothrow: true })) === null) {
        const selected = await vscode.window.showInformationMessage(
          "Install mq-lsp-server?",
          "Yes",
          "No"
        );

        if (selected === "Yes") {
          await installLspServer();
          await startLspServer();
        } else {
          vscode.window.showErrorMessage("mq-lsp not found in PATH");
        }
      } else {
        await startLspServer();
      }
    }
  }
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

const startLspServer = async () => {
  if (client !== null) {
    return;
  }

  let lspPath: string | null;

  if (process.env._MQ_DEBUG_BIN) {
    lspPath = process.env._MQ_DEBUG_BIN;
  } else {
    const config = vscode.workspace.getConfiguration("mq-lsp");
    const configLspPath = config.get<string>("lspPath");

    if (configLspPath) {
      lspPath = configLspPath;
    } else {
      lspPath = await which("mq-lsp", { nothrow: true });
    }

    if (lspPath === null) {
      vscode.window.showErrorMessage("mq-lsp not found in PATH");
      return;
    }
  }

  const run: lc.Executable = {
    command: lspPath,
    args: [],
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
  };

  client = new lc.LanguageClient(
    "mq",
    "mq Language Server",
    serverOptions,
    clientOptions
  );

  return client.start();
};

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
    // new vscode.ShellExecution("cargo install mq-lsp")
    new vscode.ShellExecution(
      "cargo install --git https://github.com/harehare/mq.git mq-lsp"
    )
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
