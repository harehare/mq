import * as assert from "assert";
import * as vscode from "vscode";

const EXTENSION_ID = "harehare.vscode-mq";

// Registered in extension.ts but deliberately left out of package.json's
// contributes.commands so they don't clutter the Command Palette (e.g.
// only triggered from the status bar item or another command).
const INTERNAL_ONLY_COMMANDS = [
  "mq.stopLSPServer",
  "mq.restartLSPServer",
  "mq.showLspStatus",
  "mq.showLSPOutput",
];

suite("mq extension", () => {
  let ext: vscode.Extension<unknown>;

  suiteSetup(async function () {
    this.timeout(60_000);
    const found = vscode.extensions.getExtension(EXTENSION_ID);
    assert.ok(found, `extension ${EXTENSION_ID} not found`);
    ext = found;
    await ext.activate();
  });

  test("activates successfully", () => {
    assert.strictEqual(ext.isActive, true);
  });

  test("registers every command declared in package.json plus internal-only commands", async () => {
    const declaredCommands: string[] = ext.packageJSON.contributes.commands.map(
      (c: { command: string }) => c.command,
    );
    assert.ok(
      declaredCommands.length > 0,
      "expected package.json to declare at least one command",
    );

    const registered = await vscode.commands.getCommands(true);
    for (const command of [...declaredCommands, ...INTERNAL_ONLY_COMMANDS]) {
      assert.ok(registered.includes(command), `command ${command} was not registered`);
    }
  });

  test("mq.new opens an untitled document with the mq language", async () => {
    await vscode.commands.executeCommand("mq.new");
    const editor = vscode.window.activeTextEditor;
    assert.ok(editor, "expected an active editor after mq.new");
    assert.strictEqual(editor!.document.languageId, "mq");
  });

  test("mq language is registered", async () => {
    const languages = await vscode.languages.getLanguages();
    assert.ok(languages.includes("mq"));
  });
});
