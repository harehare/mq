import * as lc from "vscode-languageclient/node";
import * as vscode from "vscode";
import which from "which";

const MQ_VERSION_KEY = "mq.version" as const;
const COMMANDS = ["mq/run"] as const;

const EXAMPLES = `# To hide these examples, set mq.showExamplesInNewFile to false in settings
# Hello world
def hello_world():
  add(" Hello World")?;
select(or(.[], .code, .h)) | upcase() | hello_world();

# Extract js code
.code("js")

# Extract list
.[]

# Extract table
.[][]

# Extract MDX
select(is_mdx())

# Custom function
def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
    let first_char = upcase(first(word))
    | let rest_str = downcase(slice(word, 1, len(word)))
    | s"\${first_char}\${rest_str}";
  | join("");
| snake_to_camel()

# Markdown Toc
.h
| let link = to_link(add("#", to_text(self)), to_text(self), "")
| if (is_h1()):
  to_md_list(link, 1)
elif (is_h2()):
  to_md_list(link, 2)
elif (is_h3()):
  to_md_list(link, 3)
elif (is_h4()):
  to_md_list(link, 4)
elif (is_h5()):
  to_md_list(link, 5)
else:
  None;
`;

let client: lc.LanguageClient | null = null;

export async function activate(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.commands.registerCommand("mq.new", async () => {
      const config = vscode.workspace.getConfiguration("mq");
      const showExamplesInNewFile = config.get<boolean>(
        "showExamplesInNewFile"
      );
      const document = await vscode.workspace.openTextDocument({
        language: "mq",
        content: showExamplesInNewFile ? EXAMPLES : "",
      });
      await vscode.window.showTextDocument(document);
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("mq.installLSPServer", async () => {
      await stopLspServer();
      await installLspServer(context, true);
      await startLspServer();
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("mq.startLSPServer", async () => {
      if (client) {
        await client.stop();
        client = null;
      }
      await stopLspServer();
      await startLspServer();
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("mq.runSelectedText", async () => {
      const command = selectedText();

      if (!command) {
        return;
      }

      const editor = vscode.window.activeTextEditor;

      if (!editor) {
        vscode.window.showErrorMessage("No active editor");
        return null;
      }

      const mdFiles = await vscode.workspace.findFiles("**/*.md");

      if (mdFiles.length === 0) {
        vscode.window.showInformationMessage("No .md files found in workspace");
        return;
      }

      const items = mdFiles.map((uri) => {
        const relativePath = vscode.workspace.asRelativePath(uri);
        const fileName = uri.fsPath.split(/[/\\]/).pop() || relativePath;
        return {
          label: fileName,
          description: relativePath,
          uri,
        };
      });

      const selectedItem = await vscode.window.showQuickPick(items, {
        placeHolder: "Select a .md file as input",
      });

      if (selectedItem) {
        const document = await vscode.workspace.openTextDocument(
          selectedItem.uri
        );
        return await executeCommand("mq/run", command, document.getText());
      } else {
        return await vscode.window.showInformationMessage("No file selected");
      }
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("mq.executeMqFile", async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor) {
        vscode.window.showErrorMessage("No active editor");
        return;
      }

      const mqFiles = await vscode.workspace.findFiles("**/*.mq");

      if (mqFiles.length === 0) {
        vscode.window.showInformationMessage("No .mq files found in workspace");
        return;
      }

      const currentFileText = editor.document.getText();

      const items = mqFiles.map((uri) => {
        const relativePath = vscode.workspace.asRelativePath(uri);
        const fileName = uri.fsPath.split(/[/\\]/).pop() || relativePath;
        return {
          label: fileName,
          description: relativePath,
          uri,
        };
      });

      const selectedItem = await vscode.window.showQuickPick(items, {
        placeHolder: "Select a .mq file to execute",
      });

      if (selectedItem) {
        const document = await vscode.workspace.openTextDocument(
          selectedItem.uri
        );
        await executeCommand("mq/run", document.getText(), currentFileText);
      } else {
        await vscode.window.showInformationMessage("No file selected");
      }
    })
  );

  if (process.env._MQ_DEBUG_BIN) {
    await startLspServer();
  } else {
    const config = vscode.workspace.getConfiguration("mq");
    const configLspPath = config.get<string>("path");

    if (configLspPath) {
      await startLspServer();
    } else if ((await which("mq", { nothrow: true })) === null) {
      const selected = await vscode.window.showInformationMessage(
        "Install mq?",
        "Yes",
        "No"
      );

      if (selected === "Yes") {
        await installLspServer(context, false);
        await startLspServer();
      } else {
        vscode.window.showErrorMessage("mq not found in PATH");
      }
    } else {
      const prevVersion = context.globalState.get<string>(MQ_VERSION_KEY);
      const currentVersion = context.extension.packageJSON.version;

      if (!prevVersion || currentVersion !== prevVersion) {
        const selected = await vscode.window.showInformationMessage(
          `mq has been updated. Would you like to install the latest version?`,
          "Yes",
          "No"
        );

        if (selected === "Yes") {
          await installLspServer(context, false);
        } else {
          await context.globalState.update(MQ_VERSION_KEY, currentVersion);
        }
      }

      await startLspServer();
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
  script: string,
  input: string
) => {
  if (!client) {
    vscode.window.showErrorMessage("LSP server is not running");
    return;
  }

  try {
    const result = await client.sendRequest(lc.ExecuteCommandRequest.type, {
      command,
      arguments: [script, input],
    });

    if (result) {
      const outputChannel = vscode.window.createOutputChannel(
        "mq LSP Output",
        "markdown"
      );
      outputChannel.clear();
      outputChannel.appendLine(result);
      outputChannel.show();
    } else {
      await vscode.window.showErrorMessage("No result from LSP server");
    }
  } catch (error) {
    await vscode.window.showErrorMessage(
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
    const config = vscode.workspace.getConfiguration("mq");
    const configLspPath = config.get<string>("lspPath");

    if (configLspPath) {
      lspPath = configLspPath;
    } else {
      lspPath = await which("mq", { nothrow: true });
    }

    if (lspPath === null) {
      vscode.window.showErrorMessage("mq not found in PATH");
      return;
    }
  }

  const run: lc.Executable = {
    command: lspPath,
    args: ["lsp"],
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

const installLspServer = async (
  context: vscode.ExtensionContext,
  force: boolean
) => {
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
      `cargo install --git https://github.com/harehare/mq.git mq-cli${
        force ? " --force" : ""
      }`
    )
  );

  try {
    const execution = await vscode.tasks.executeTask(task);
    await context.globalState.update(
      MQ_VERSION_KEY,
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
};
