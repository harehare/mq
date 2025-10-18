import * as vscode from "vscode";
import { State } from "vscode-languageclient";

/**
 * Manages the LSP server status bar item
 * Only shown when an mq file is active
 */
export class LspStatusBarManager {
  private statusBarItem: vscode.StatusBarItem;
  private currentState: State = State.Stopped;

  constructor() {
    this.statusBarItem = vscode.window.createStatusBarItem(
      vscode.StatusBarAlignment.Right,
      100
    );
    this.statusBarItem.command = "mq.showLspStatus";
    this.updateStatusBar(State.Stopped);
  }

  /**
   * Update the status bar based on LSP server state
   */
  updateStatusBar(state: State): void {
    this.currentState = state;

    switch (state) {
      case State.Starting:
        this.statusBarItem.text = "$(sync~spin) mq LSP";
        this.statusBarItem.tooltip = "mq Language Server is starting...";
        this.statusBarItem.backgroundColor = undefined;
        break;
      case State.Running:
        this.statusBarItem.text = "$(check) mq LSP";
        this.statusBarItem.tooltip = "mq Language Server is running";
        this.statusBarItem.backgroundColor = undefined;
        break;
      case State.Stopped:
        this.statusBarItem.text = "$(debug-disconnect) mq LSP";
        this.statusBarItem.tooltip =
          "mq Language Server is stopped. Click to start.";
        this.statusBarItem.backgroundColor = new vscode.ThemeColor(
          "statusBarItem.warningBackground"
        );
        break;
      default:
        this.statusBarItem.text = "$(warning) mq LSP";
        this.statusBarItem.tooltip = "mq Language Server status unknown";
        this.statusBarItem.backgroundColor = new vscode.ThemeColor(
          "statusBarItem.errorBackground"
        );
    }
  }

  /**
   * Update visibility based on active editor
   */
  updateVisibility(editor: vscode.TextEditor | undefined): void {
    if (editor && editor.document.languageId === "mq") {
      this.statusBarItem.show();
    } else {
      this.statusBarItem.hide();
    }
  }

  /**
   * Get the current LSP server state
   */
  getState(): State {
    return this.currentState;
  }

  /**
   * Dispose the status bar item
   */
  dispose(): void {
    this.statusBarItem.dispose();
  }

  /**
   * Show status options menu
   */
  async showStatusMenu(): Promise<void> {
    const items: vscode.QuickPickItem[] = [
      {
        label: "$(debug-start) Start LSP Server",
        description: "Start the mq Language Server",
      },
      {
        label: "$(info) Show Output",
        description: "Show LSP server output channel",
      },
    ];

    const selected = await vscode.window.showQuickPick(items, {
      placeHolder: `LSP Server Status: ${State[this.currentState]}`,
    });

    if (!selected) {
      return;
    }

    switch (selected.label) {
      case "$(debug-start) Start LSP Server":
        await vscode.commands.executeCommand("mq.startLSPServer");
        break;
      case "$(info) Show Output":
        await vscode.commands.executeCommand("mq.showLSPOutput");
        break;
    }
  }
}
