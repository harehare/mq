import * as lc from "vscode-languageclient/node";
import * as vscode from "vscode";
import which from "which";

const COMMANDS = [
  "mq/setSelectedTextAsInput",
  "mq/runSelectedText",
  "mq/showInputText",
] as const;
let client: lc.LanguageClient | null = null;

export async function activate(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.commands.registerCommand("mq-lsp.installLSPServer", async () => {
      await stopLspServer();
      await installLspServer(true);
      await startLspServer();
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("mq-lsp.startLSPServer", async () => {
      if (client) {
        await client.stop();
        client = null;
      }
      await stopLspServer();
      await startLspServer();
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("mq-lsp.runSelectedText", async () => {
      const text = selectedText();

      if (!text) {
        return;
      }

      await executeCommand("mq/runSelectedText", text);
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      "mq-lsp.setSelectedTextAsInput",
      async () => {
        const text = selectedText();

        if (!text) {
          return;
        }

        await executeCommand("mq/setSelectedTextAsInput", text);
      }
    )
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("mq-lsp.showInput", async () => {
      await executeCommand("mq/showInputText");
    })
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
          await installLspServer(false);
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

const selectedText = () => {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    vscode.window.showErrorMessage("No active editor");
    return null;
  }

  const selection = editor.selection;
  const selectedText = editor.document.getText(selection);

  if (!selectedText) {
    vscode.window.showErrorMessage("No text selected");
    return null;
  }

  return selectedText;
};

const executeCommand = async (
  command: (typeof COMMANDS)[number],
  text?: string
) => {
  if (!client) {
    vscode.window.showErrorMessage("LSP server is not running");
    return;
  }

  try {
    const result = await client.sendRequest(lc.ExecuteCommandRequest.type, {
      command,
      arguments: [text],
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
};

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

const stopLspServer = async () => {
  if (!client) {
    return undefined;
  }
  await client.stop();
  client = null;
};

const installLspServer = async (force: boolean) => {
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
      `cargo install --git https://github.com/harehare/mq.git mq-lsp${
        force ? " --force" : ""
      }`
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
