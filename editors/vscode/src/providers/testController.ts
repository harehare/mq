import { execFile } from "node:child_process";
import path from "node:path";
import { promisify } from "node:util";
import * as vscode from "vscode";
import which from "which";

const execFileAsync = promisify(execFile);

interface ListedTest {
  name: string;
  kind: "simple" | "parametrized" | "property";
  tags: string[];
}

interface ListedFile {
  file: string;
  tests: ListedTest[];
}

interface ListReport {
  files: ListedFile[];
}

interface RunTestResult {
  name: string;
  status: "passed" | "failed";
  duration: number;
  error: string | null;
}

interface RunFileResult {
  file: string;
  tests?: RunTestResult[];
  error?: string;
}

interface RunReport {
  files: RunFileResult[];
}

interface ExecError extends Error {
  stdout?: string;
  stderr?: string;
}

/**
 * A `# @parametrize(...)`/`# @property(...)` test is listed once (by `mq-test --list`) but
 * runs as several `name[0]`, `name[1]`, ... cases (see `run` in `mq-test`'s generated query).
 * This strips that `[...]` suffix so a run result can be matched back to the single TestItem
 * `discoverTests` created for the function.
 */
function baseTestName(name: string): string {
  return name.replace(/\[[^\]]*\]$/, "");
}

async function resolveMqTestPath(): Promise<string | null> {
  const config = vscode.workspace.getConfiguration("mq");
  const configuredPath = config.get<string>("testPath");
  if (configuredPath) {
    return configuredPath;
  }
  return await which("mq-test", { nothrow: true });
}

function showMqTestNotFoundError() {
  vscode.window.showErrorMessage(
    "mq-test not found. Install it with `cargo install mq-test`, or set mq.testPath in settings.",
  );
}

/**
 * Registers a `TestController` that discovers mq tests via `mq-test --list --format json` and
 * runs them via `mq-test <file> --format json`, mapping each file's Markdown-report-free JSON
 * output onto the corresponding `TestItem`s.
 */
export function registerMqTestController(
  context: vscode.ExtensionContext,
): vscode.TestController {
  const controller = vscode.tests.createTestController("mq", "mq Tests");
  context.subscriptions.push(controller);

  // Keyed by the owning file's `TestItem.id` (its file URI string), so a run request's
  // included items (which may be file items or individual test items) can be traced back
  // to the file that needs to be invoked, and a run's per-test JSON results matched back to
  // their `TestItem`s.
  const fileItems = new Map<string, vscode.TestItem>();

  controller.resolveHandler = async (item) => {
    if (item) {
      // Every test is discovered up front by `discoverTests`, so there's nothing further
      // to resolve for an individual item.
      return;
    }
    await discoverTests(controller, fileItems);
  };
  controller.refreshHandler = async () => {
    await discoverTests(controller, fileItems);
  };

  const runProfile = controller.createRunProfile(
    "Run",
    vscode.TestRunProfileKind.Run,
    (request, token) => runHandler(controller, fileItems, request, token),
    true,
  );
  context.subscriptions.push(runProfile);

  return controller;
}

async function discoverTests(
  controller: vscode.TestController,
  fileItems: Map<string, vscode.TestItem>,
) {
  const workspaceFolders = vscode.workspace.workspaceFolders;
  if (!workspaceFolders || workspaceFolders.length === 0) {
    return;
  }

  const testPath = await resolveMqTestPath();
  if (!testPath) {
    showMqTestNotFoundError();
    return;
  }

  controller.items.replace([]);
  fileItems.clear();

  for (const folder of workspaceFolders) {
    let report: ListReport;
    try {
      const { stdout } = await execFileAsync(
        testPath,
        ["--list", "--format", "json"],
        { cwd: folder.uri.fsPath, encoding: "utf8" },
      );
      report = JSON.parse(stdout);
    } catch (error) {
      vscode.window.showErrorMessage(
        `mq-test --list failed: ${error instanceof Error ? error.message : "Unknown error"}`,
      );
      continue;
    }

    for (const fileEntry of report.files) {
      const absolutePath = path.isAbsolute(fileEntry.file)
        ? fileEntry.file
        : path.join(folder.uri.fsPath, fileEntry.file);
      const fileUri = vscode.Uri.file(absolutePath);

      const fileItem = controller.createTestItem(
        fileUri.toString(),
        vscode.workspace.asRelativePath(fileUri),
        fileUri,
      );
      controller.items.add(fileItem);
      fileItems.set(fileItem.id, fileItem);

      for (const test of fileEntry.tests) {
        const testItem = controller.createTestItem(
          `${fileUri.toString()}::${test.name}`,
          test.name,
          fileUri,
        );
        testItem.tags = test.tags.map((tag) => new vscode.TestTag(tag));
        fileItem.children.add(testItem);
      }
    }
  }
}

async function runHandler(
  controller: vscode.TestController,
  fileItems: Map<string, vscode.TestItem>,
  request: vscode.TestRunRequest,
  token: vscode.CancellationToken,
) {
  const run = controller.createTestRun(request);
  const workspaceFolders = vscode.workspace.workspaceFolders;

  if (!workspaceFolders || workspaceFolders.length === 0) {
    run.end();
    return;
  }

  const testPath = await resolveMqTestPath();
  if (!testPath) {
    showMqTestNotFoundError();
    run.end();
    return;
  }

  const filesToRun = resolveFilesToRun(request, fileItems);

  for (const fileItem of filesToRun) {
    if (token.isCancellationRequested) {
      break;
    }
    if (request.exclude?.includes(fileItem) || !fileItem.uri) {
      continue;
    }

    await runFile(testPath, fileItem, fileItem.uri, request, workspaceFolders, run);
  }

  run.end();
}

// A run can include file items, individual test items, or (when nothing is included) every
// known file — resolve all of that down to the set of file items actually invoked, since
// `mq-test` runs a whole file at a time.
function resolveFilesToRun(
  request: vscode.TestRunRequest,
  fileItems: Map<string, vscode.TestItem>,
): Set<vscode.TestItem> {
  const included = request.include ?? [...fileItems.values()];
  const filesToRun = new Set<vscode.TestItem>();
  for (const item of included) {
    filesToRun.add(item.parent ?? item);
  }
  return filesToRun;
}

async function runFile(
  testPath: string,
  fileItem: vscode.TestItem,
  fileUri: vscode.Uri,
  request: vscode.TestRunRequest,
  workspaceFolders: readonly vscode.WorkspaceFolder[],
  run: vscode.TestRun,
) {
  const childItems = [...fileItem.children].map(([, child]) => child);
  const runnableChildren = childItems.filter((child) => !request.exclude?.includes(child));
  for (const child of runnableChildren) {
    run.enqueued(child);
  }

  const folder = vscode.workspace.getWorkspaceFolder(fileUri);
  const cwd = folder?.uri.fsPath ?? workspaceFolders[0].uri.fsPath;

  for (const child of runnableChildren) {
    run.started(child);
  }

  const execution = await executeMqTest(testPath, fileUri.fsPath, cwd);
  if ("errorMessage" in execution) {
    const message = new vscode.TestMessage(execution.errorMessage);
    for (const child of runnableChildren) {
      run.errored(child, message);
    }
    return;
  }

  const fileResult = execution.report.files.find(
    (f) => path.resolve(cwd, f.file) === fileUri.fsPath,
  );
  if (!fileResult) {
    return;
  }

  if (fileResult.error) {
    const message = new vscode.TestMessage(fileResult.error);
    for (const child of runnableChildren) {
      run.errored(child, message);
    }
    return;
  }

  reportFileResults(fileResult, runnableChildren, run);
}

type MqTestExecution = { report: RunReport } | { errorMessage: string };

async function executeMqTest(
  testPath: string,
  filePath: string,
  cwd: string,
): Promise<MqTestExecution> {
  try {
    const { stdout } = await execFileAsync(testPath, [filePath, "--format", "json"], {
      cwd,
      encoding: "utf8",
    });
    return { report: JSON.parse(stdout) };
  } catch (error) {
    // `mq-test` exits non-zero when any test in the file fails, so a failing (but
    // otherwise healthy) run still needs its JSON report read from the rejected
    // promise's `stdout`, not treated as `mq-test` itself having errored.
    const execError = error as ExecError;
    const parsed = execError.stdout ? tryParseJson<RunReport>(execError.stdout) : null;
    return parsed ? { report: parsed } : { errorMessage: execError.message };
  }
}

// Group by base name so a parametrized/property test's several `name[i]` cases all report
// onto the one TestItem `discoverTests` created for the function (see `baseTestName`).
function reportFileResults(
  fileResult: RunFileResult,
  runnableChildren: vscode.TestItem[],
  run: vscode.TestRun,
) {
  const resultsByBaseName = new Map<string, RunTestResult[]>();
  for (const testResult of fileResult.tests ?? []) {
    const key = baseTestName(testResult.name);
    const cases = resultsByBaseName.get(key) ?? [];
    cases.push(testResult);
    resultsByBaseName.set(key, cases);
  }

  for (const child of runnableChildren) {
    const cases = resultsByBaseName.get(child.label);
    if (!cases) {
      continue;
    }

    const totalDuration = cases.reduce((sum, c) => sum + c.duration, 0);
    const failures = cases.filter((c) => c.status === "failed");
    if (failures.length === 0) {
      run.passed(child, totalDuration);
    } else {
      const message = new vscode.TestMessage(
        failures
          .map((f) => (f.name === child.label ? f.error : `${f.name}: ${f.error}`))
          .join("\n\n"),
      );
      run.failed(child, message, totalDuration);
    }
  }
}

function tryParseJson<T>(text: string): T | null {
  try {
    return JSON.parse(text) as T;
  } catch {
    return null;
  }
}
