package org.mqlang.mq.actions

import com.google.gson.Gson
import com.intellij.execution.ProgramRunnerUtil
import com.intellij.execution.RunManager
import com.intellij.execution.configurations.ConfigurationType
import com.intellij.execution.executors.DefaultDebugExecutor
import com.intellij.openapi.actionSystem.ActionUpdateThread
import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.actionSystem.CommonDataKeys
import com.intellij.openapi.fileEditor.FileDocumentManager
import com.intellij.openapi.project.Project
import com.intellij.openapi.ui.Messages
import com.redhat.devtools.lsp4ij.dap.DebugMode
import com.redhat.devtools.lsp4ij.dap.configurations.DAPRunConfiguration
import org.mqlang.mq.install.MqBinaryLocator

private const val DAP_CONFIGURATION_TYPE_ID = "DAPConfiguration"
private const val MQ_DAP_SERVER_ID = "mq"

/** `mq: Debug current file` — launches an mq-dbg Debug Adapter Protocol session for the active `.mq` file. */
class MqDebugCurrentFileAction : AnAction() {

    override fun getActionUpdateThread(): ActionUpdateThread = ActionUpdateThread.BGT

    override fun update(e: AnActionEvent) {
        val editor = e.getData(CommonDataKeys.EDITOR)
        val file = editor?.let { FileDocumentManager.getInstance().getFile(it.document) }
        e.presentation.isEnabledAndVisible = file?.extension == "mq"
    }

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val editor = e.getData(CommonDataKeys.EDITOR) ?: return
        val queryFile = FileDocumentManager.getInstance().getFile(editor.document) ?: return

        if (editor.document.isWritable && FileDocumentManager.getInstance().isDocumentUnsaved(editor.document)) {
            FileDocumentManager.getInstance().saveDocument(editor.document)
        }

        if (MqBinaryLocator.findDbgPath() == null) {
            Messages.showErrorDialog(
                project,
                "mq-dbg not found. Install it via the \"mq: Install Servers\" action or set its path in Settings > mq.",
                "mq",
            )
            return
        }

        pickInputFile(project) { inputFile ->
            startDebugSession(project, queryFile.path, inputFile.path)
        }
    }

    private fun startDebugSession(project: Project, queryFilePath: String, inputFilePath: String) {
        val configType = ConfigurationType.CONFIGURATION_TYPE_EP.extensionList
            .firstOrNull { it.id == DAP_CONFIGURATION_TYPE_ID }
        if (configType == null) {
            Messages.showErrorDialog(project, "The LSP4IJ Debug Adapter Protocol run configuration type was not found.", "mq")
            return
        }
        val factory = configType.configurationFactories.first()

        val runManager = RunManager.getInstance(project)
        val settings = runManager.createConfiguration("Debug mq query", factory)
        val configuration = settings.configuration
        if (configuration !is DAPRunConfiguration) {
            Messages.showErrorDialog(project, "Unexpected Debug Adapter Protocol run configuration type.", "mq")
            return
        }

        configuration.serverId = MQ_DAP_SERVER_ID
        configuration.serverName = "mq"
        configuration.debugMode = DebugMode.LAUNCH
        configuration.file = queryFilePath
        configuration.launchConfiguration = Gson().toJson(
            mapOf(
                "type" to "mq",
                "name" to "Debug mq query",
                "request" to "launch",
                "queryFile" to queryFilePath,
                "inputFile" to inputFilePath,
            ),
        )

        runManager.addConfiguration(settings)
        runManager.selectedConfiguration = settings
        ProgramRunnerUtil.executeConfiguration(settings, DefaultDebugExecutor.getDebugExecutorInstance())
    }
}
