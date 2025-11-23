import * as lc from "vscode-languageclient/node";
import * as vscode from "vscode";
import which from "which";
import { MqDebugConfigurationProvider } from "./providers/debugger";
import { MqCodeLensProvider } from "./providers/codelens";
import { LspStatusBarManager } from "./providers/statusbar";

const MQ_VERSION_KEY = "mq.version" as const;
const COMMANDS = ["mq/run"] as const;

const EXAMPLES = `# To hide these examples, set mq.showExamplesInNewFile to false in settings
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
| let link = to_link("#" + to_text(self), to_text(self), "")
| let level = .h.depth
| if (!is_none(level)): to_md_list(link, to_number(level))

# CSV parse
include "csv" | csv_parse("a,b,c\n1,2,3\n4,5,6", true) | csv_to_markdown_table()
`;

let client: lc.LanguageClient | null = null;
let codeLensProvider: vscode.Disposable | null = null;
let statusBarManager: LspStatusBarManager | null = null;

const InputFormatMap = {
  md: "markdown",
  mdx: "mdx",
  html: "html",
  txt: "text",
} as const;
type InputFormatExtension = keyof typeof InputFormatMap;
type InputFormat = (typeof InputFormatMap)[keyof typeof InputFormatMap];

interface FilePickerItem {
  label: string;
  description: string;
  uri: vscode.Uri;
}

// Helper functions
function getInputFormatFromExtension(extension: string): InputFormat {
  const formatKey = (
    Object.keys(InputFormatMap) as Array<InputFormatExtension>
  ).includes(extension as InputFormatExtension)
    ? (extension as InputFormatExtension)
    : "md";
  return InputFormatMap[formatKey];
}

function createFilePickerItems(files: vscode.Uri[]): FilePickerItem[] {
  return files.map((uri) => {
    const relativePath = vscode.workspace.asRelativePath(uri);
    const fileName = uri.fsPath.split(/[/\\]/).pop() || relativePath;
    return {
      label: fileName,
      description: relativePath,
      uri,
    };
  });
}

async function selectMarkdownFile(): Promise<{
  document: vscode.TextDocument;
  inputFormat: InputFormat;
} | null> {
  const mdFiles = await vscode.workspace.findFiles(
    "**/*.{md,mdx,html,csv,tsv,txt}"
  );

  if (mdFiles.length === 0) {
    vscode.window.showInformationMessage(
      "No .md, .mdx, .html, .csv, .tsv, or .txt files found in workspace"
    );
    return null;
  }

  const items = createFilePickerItems(mdFiles);
  const selectedItem = await vscode.window.showQuickPick(items, {
    placeHolder: "Select a .md, .mdx, .html, .csv, .tsv or .txt file as input",
  });

  if (!selectedItem) {
    return null;
  }

  const document = await vscode.workspace.openTextDocument(selectedItem.uri);
  const extension = selectedItem.uri.fsPath.split(".").pop() || "";
  const inputFormat = getInputFormatFromExtension(extension);

  return { document, inputFormat };
}

function getActiveEditorValidation(): vscode.TextEditor | null {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    vscode.window.showErrorMessage("No active editor");
    return null;
  }
  return editor;
}

// Command registration functions
function registerNewCommand(context: vscode.ExtensionContext) {
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
}

function registerLspCommands(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.commands.registerCommand("mq.installServers", async () => {
      await stopLspServer();
      await installServers(context, true);
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
    vscode.commands.registerCommand("mq.stopLSPServer", async () => {
      await stopLspServer();
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("mq.restartLSPServer", async () => {
      await stopLspServer();
      await startLspServer();
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("mq.showLspStatus", async () => {
      if (statusBarManager) {
        await statusBarManager.showStatusMenu();
      }
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("mq.showLSPOutput", () => {
      if (client) {
        client.outputChannel?.show();
      }
    })
  );
}

function registerDebugCommands(context: vscode.ExtensionContext) {
  const provider = new MqDebugConfigurationProvider(context);
  context.subscriptions.push(
    vscode.debug.registerDebugConfigurationProvider("mq", provider)
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("mq.debugCurrentFile", async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor) {
        vscode.window.showErrorMessage("No active editor");
        return;
      }

      if (editor.document.languageId !== "mq") {
        vscode.window.showErrorMessage("Active file is not a .mq file");
        return;
      }

      if (editor.document.isDirty) {
        await editor.document.save();
      }

      const selectedFile = await selectMarkdownFile();
      if (!selectedFile) {
        return;
      }

      const config: vscode.DebugConfiguration = {
        type: "mq",
        name: "Debug Current File",
        request: "launch",
        queryFile: editor.document.uri.fsPath,
        inputFile: selectedFile.document.uri.fsPath,
      };

      const success = await vscode.debug.startDebugging(undefined, config);
      if (!success) {
        vscode.window.showErrorMessage(
          "Failed to start debugging. Please ensure mq-dbg is installed and the configuration is correct."
        );
      }
    })
  );
}

function registerMqExecutionCommands(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.commands.registerCommand("mq.runSelectedText", async () => {
      const command = selectedText();
      if (!command) {
        return;
      }

      const selectedFile = await selectMarkdownFile();
      if (!selectedFile) {
        return;
      }

      return await executeCommand(
        "mq/run",
        command,
        selectedFile.document.getText(),
        selectedFile.inputFormat
      );
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("mq.executeMqQuery", async () => {
      const editor = getActiveEditorValidation();
      if (!editor) {
        return;
      }

      const query = await vscode.window.showInputBox({
        prompt: "Enter mq query to execute",
        placeHolder: "e.g. .[] | upcase()",
      });

      if (!query) {
        vscode.window.showErrorMessage("No query entered");
        return;
      }

      const extension = editor.document.uri.fsPath.split(".").pop() || "";
      const inputFormat = getInputFormatFromExtension(extension);

      await executeCommand(
        "mq/run",
        query,
        editor.document.getText(),
        inputFormat
      );
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("mq.executeMqFile", async () => {
      const editor = getActiveEditorValidation();
      if (!editor) {
        return;
      }

      const mqFiles = await vscode.workspace.findFiles("**/*.mq");
      if (mqFiles.length === 0) {
        vscode.window.showInformationMessage("No .mq files found in workspace");
        return;
      }

      const items = createFilePickerItems(mqFiles);
      const selectedItem = await vscode.window.showQuickPick(items, {
        placeHolder: "Select a .mq file to execute",
      });

      if (!selectedItem) {
        await vscode.window.showInformationMessage("No file selected");
        return;
      }

      const document = await vscode.workspace.openTextDocument(
        selectedItem.uri
      );
      const extension = editor.document.uri.fsPath.split(".").pop() || "";
      const inputFormat = getInputFormatFromExtension(extension);

      await executeCommand(
        "mq/run",
        document.getText(),
        editor.document.getText(),
        inputFormat
      );
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      "mq.runQueryAndShowInEditor",
      async (query: string) => {
        const selectedFile = await selectMarkdownFile();
        if (!selectedFile) {
          return;
        }

        return await executeCommand(
          "mq/run",
          query,
          selectedFile.document.getText(),
          selectedFile.inputFormat
        );
      }
    )
  );

  // Initialize Code Lens provider
  updateCodeLensProvider(context);

  // Watch for configuration changes
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("mq.enableCodeLens")) {
        updateCodeLensProvider(context);
      }
    })
  );
}

function updateCodeLensProvider(context: vscode.ExtensionContext) {
  const config = vscode.workspace.getConfiguration("mq");
  const enableCodeLens = config.get<boolean>("enableCodeLens", true);

  // Dispose existing provider if it exists
  if (codeLensProvider) {
    codeLensProvider.dispose();
    codeLensProvider = null;
  }

  // Register new provider if enabled
  if (enableCodeLens) {
    codeLensProvider = vscode.languages.registerCodeLensProvider(
      { language: "mq" },
      new MqCodeLensProvider()
    );
    context.subscriptions.push(codeLensProvider);
  }
}

async function initializeLspServer(context: vscode.ExtensionContext) {
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
        await installServers(context, false);
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
          await installServers(context, false);
        } else {
          await context.globalState.update(MQ_VERSION_KEY, currentVersion);
        }
      }

      await startLspServer();
    }
  }
}

export async function activate(context: vscode.ExtensionContext) {
  // Initialize status bar manager
  statusBarManager = new LspStatusBarManager();
  context.subscriptions.push(statusBarManager);

  // Update status bar visibility when active editor changes
  context.subscriptions.push(
    vscode.window.onDidChangeActiveTextEditor((editor) => {
      statusBarManager?.updateVisibility(editor);
    })
  );

  // Set initial visibility
  statusBarManager.updateVisibility(vscode.window.activeTextEditor);

  registerNewCommand(context);
  registerLspCommands(context);
  registerDebugCommands(context);
  registerMqExecutionCommands(context);
  await initializeLspServer(context);
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
  input: string,
  inputFormat: InputFormat
) => {
  if (!client) {
    vscode.window.showErrorMessage("LSP server is not running");
    return;
  }

  try {
    const result = await client.sendRequest(lc.ExecuteCommandRequest.type, {
      command,
      arguments: [script, input, inputFormat],
    });

    if (result) {
      const outputChannel = vscode.window.createOutputChannel(
        "mq LSP Output",
        "markdown"
      );
      outputChannel.clear();
      outputChannel.appendLine(result);
      outputChannel.show();

      const copyAction = "Copy result to clipboard";
      const selection = await vscode.window.showInformationMessage(
        "mq executed.",
        copyAction
      );

      if (selection === copyAction) {
        try {
          await vscode.env.clipboard.writeText(result);
          vscode.window.showInformationMessage("Result copied to clipboard.");
        } catch {
          vscode.window.showErrorMessage("Failed to copy result to clipboard.");
        }
      }
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
      statusBarManager?.updateStatusBar(lc.State.Stopped);
      return;
    }
  }

  const workspaceFolders = vscode.workspace.workspaceFolders;
  const workspacePaths =
    workspaceFolders && workspaceFolders.length > 0
      ? workspaceFolders.map((folder) => folder.uri.fsPath)
      : [process.cwd()];

  const multiWorkspaceArgs = workspacePaths.flatMap((path) => ["-M", path]);

  const run: lc.Executable = {
    command: lspPath,
    args: ["lsp", ...multiWorkspaceArgs],
    options: {},
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

  // Update status bar when client state changes
  client.onDidChangeState((event) => {
    statusBarManager?.updateStatusBar(event.newState);
  });

  statusBarManager?.updateStatusBar(lc.State.Starting);
  await client.start();
  statusBarManager?.updateStatusBar(lc.State.Running);
};

const stopLspServer = async () => {
  if (!client) {
    return undefined;
  }
  await client.stop();
  client = null;
  statusBarManager?.updateStatusBar(lc.State.Stopped);
};

const installServers = async (
  context: vscode.ExtensionContext,
  force: boolean
) => {
  const cargoPath = await which("cargo", { nothrow: true });

  if (cargoPath === null) {
    vscode.window.showErrorMessage("Cargo not found in PATH");
    return false;
  }

  const installLspCommand = `cargo install --git https://github.com/harehare/mq.git mq-run ${
    force ? " --force" : ""
  }`;
  const installDapCommand = `cargo install --git https://github.com/harehare/mq.git mq-run --bin mq-dbg --features="debugger" ${
    force ? " --force" : ""
  }`;

  const installTask = new vscode.Task(
    { type: "cargo", task: "install-lsp-server" },
    vscode.TaskScope.Workspace,
    "Install LSP Server",
    "mq-lsp",
    new vscode.ShellExecution(
      [installLspCommand, installDapCommand].join(" && ")
    )
  );

  try {
    // Start both tasks
    const execution = await vscode.tasks.executeTask(installTask);
    await context.globalState.update(
      MQ_VERSION_KEY,
      context.extension.packageJSON.version
    );

    // Track completion of both tasks
    return new Promise<boolean>((resolve) => {
      const disposable = vscode.tasks.onDidEndTaskProcess((e) => {
        if (e.execution === execution) {
          disposable.dispose();
          resolve(true);
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
