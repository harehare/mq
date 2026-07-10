import * as assert from "assert";
import * as vscode from "vscode";

const EXTENSION_ID = "harehare.vscode-mq";

const EXPECTED_COMMANDS = [
  "mq.new",
  "mq.installServers",
  "mq.startLSPServer",
  "mq.stopLSPServer",
  "mq.restartLSPServer",
  "mq.showLspStatus",
  "mq.showLSPOutput",
  "mq.debugCurrentFile",
  "mq.runSelectedText",
  "mq.executeMqQuery",
  "mq.executeMqFile",
  "mq.runQueryAndShowInEditor",
];

suite("mq extension", () => {
  suiteSetup(async function () {
    this.timeout(60_000);
    const ext = vscode.extensions.getExtension(EXTENSION_ID);
    assert.ok(ext, `extension ${EXTENSION_ID} not found`);
    await ext!.activate();
  });

  test("activates successfully", () => {
    const ext = vscode.extensions.getExtension(EXTENSION_ID);
    assert.ok(ext);
    assert.strictEqual(ext!.isActive, true);
  });

  test("registers all mq commands", async () => {
    const commands = await vscode.commands.getCommands(true);
    for (const command of EXPECTED_COMMANDS) {
      assert.ok(commands.includes(command), `command ${command} was not registered`);
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
