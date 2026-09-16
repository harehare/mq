package org.mqlang.mq.actions

import com.intellij.openapi.actionSystem.ActionUpdateThread
import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.actionSystem.CommonDataKeys
import com.intellij.openapi.editor.Editor
import com.intellij.openapi.fileEditor.FileDocumentManager
import com.intellij.openapi.project.Project
import com.intellij.openapi.ui.Messages
import com.intellij.openapi.ui.popup.JBPopupFactory
import com.intellij.openapi.ui.popup.PopupStep
import com.intellij.openapi.ui.popup.util.BaseListPopupStep
import com.intellij.openapi.vfs.VirtualFile

private const val NEW_QUERY_OPTION = "New query..."

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

        val history = MqQueryHistory.load()
        if (history.isEmpty()) {
            promptAndRun(project, editor, file)
            return
        }

        val step = object : BaseListPopupStep<String>(
            "Select a recent query",
            listOf(NEW_QUERY_OPTION) + history,
        ) {
            override fun onChosen(selectedValue: String, finalChoice: Boolean): PopupStep<*>? {
                if (selectedValue == NEW_QUERY_OPTION) {
                    promptAndRun(project, editor, file)
                } else {
                    runQuery(project, editor, file, selectedValue)
                }
                return PopupStep.FINAL_CHOICE
            }
        }
        JBPopupFactory.getInstance().createListPopup(step).showInBestPositionFor(editor)
    }

    private fun promptAndRun(project: Project, editor: Editor, file: VirtualFile) {
        val query = Messages.showInputDialog(
            project,
            "Enter mq query to execute",
            "mq: Execute Query",
            null,
        )
        if (query.isNullOrBlank()) return
        runQuery(project, editor, file, query)
    }

    private fun runQuery(project: Project, editor: Editor, file: VirtualFile, query: String) {
        MqQueryHistory.add(query)
        MqCommandRunner.run(project, query, editor.document.text, MqCommandRunner.inputFormatFor(file))
    }
}
