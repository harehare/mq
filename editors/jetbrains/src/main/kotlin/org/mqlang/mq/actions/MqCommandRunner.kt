package org.mqlang.mq.actions

import com.google.gson.JsonElement
import com.intellij.notification.NotificationAction
import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.fileEditor.FileEditorManager
import com.intellij.openapi.ide.CopyPasteManager
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.testFramework.LightVirtualFile
import com.redhat.devtools.lsp4ij.commands.CommandExecutor
import com.redhat.devtools.lsp4ij.commands.LSPCommandContext
import org.eclipse.lsp4j.Command
import java.awt.datatransfer.StringSelection

/** Runs the mq-lsp custom `mq/run` workspace command, mirroring the mq VS Code extension's execution flow. */
object MqCommandRunner {

    private val INPUT_FORMAT_BY_EXTENSION = mapOf(
        "md" to "markdown",
        "mdx" to "mdx",
        "html" to "html",
        "txt" to "text",
    )

    fun inputFormatFor(file: VirtualFile): String =
        INPUT_FORMAT_BY_EXTENSION[file.extension?.lowercase()] ?: "markdown"

    fun run(project: Project, query: String, input: String, inputFormat: String) {
        val command = Command("mq/run", "mq/run", listOf(query, input, inputFormat))
        val context = LSPCommandContext(command, project)
            .setPreferredLanguageServerId("mq")
            .setShowNotificationError(true)

        val commandResponse = CommandExecutor.executeCommand(context)
        val response = commandResponse.response()
        if (!commandResponse.exists() || response == null) {
            notify(project, "mq LSP server is not running.", NotificationType.ERROR)
            return
        }

        response.thenAccept { result ->
            val text = result?.let(::asText)
            if (text.isNullOrEmpty()) {
                notify(project, "No result from LSP server", NotificationType.WARNING)
            } else {
                showResult(project, text)
            }
        }
    }

    private fun asText(result: Any): String = when (result) {
        is String -> result
        is JsonElement -> if (result.isJsonPrimitive) result.asString else result.toString()
        else -> result.toString()
    }

    private fun showResult(project: Project, text: String) {
        val file = LightVirtualFile("mq-result.md", text)
        FileEditorManager.getInstance(project).openFile(file, true)

        val notification = NotificationGroupManager.getInstance()
            .getNotificationGroup("mq")
            .createNotification("mq executed.", NotificationType.INFORMATION)
        notification.addAction(NotificationAction.createSimpleExpiring("Copy result to clipboard") {
            CopyPasteManager.getInstance().setContents(StringSelection(text))
        })
        notification.notify(project)
    }

    private fun notify(project: Project, message: String, type: NotificationType) {
        NotificationGroupManager.getInstance()
            .getNotificationGroup("mq")
            .createNotification(message, type)
            .notify(project)
    }
}
