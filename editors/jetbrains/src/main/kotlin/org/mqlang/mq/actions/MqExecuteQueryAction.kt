package org.mqlang.mq.actions

import com.intellij.openapi.actionSystem.ActionUpdateThread
import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.actionSystem.CommonDataKeys
import com.intellij.openapi.fileEditor.FileDocumentManager
import com.intellij.openapi.ui.Messages

/** `mq: Execute query` — prompts for a query, runs it against the active editor's text. */
class MqExecuteQueryAction : AnAction() {

    override fun getActionUpdateThread(): ActionUpdateThread = ActionUpdateThread.BGT

    override fun update(e: AnActionEvent) {
        e.presentation.isEnabledAndVisible = e.getData(CommonDataKeys.EDITOR) != null
    }

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val editor = e.getData(CommonDataKeys.EDITOR) ?: return
        val file = FileDocumentManager.getInstance().getFile(editor.document) ?: return

        val query = Messages.showInputDialog(
            project,
            "Enter mq query to execute",
            "mq: Execute Query",
            null,
        )
        if (query.isNullOrBlank()) return

        MqCommandRunner.run(project, query, editor.document.text, MqCommandRunner.inputFormatFor(file))
    }
}
