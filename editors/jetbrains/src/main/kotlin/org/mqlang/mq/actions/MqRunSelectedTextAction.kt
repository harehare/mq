package org.mqlang.mq.actions

import com.intellij.openapi.actionSystem.ActionUpdateThread
import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.actionSystem.CommonDataKeys
import com.intellij.openapi.ui.Messages

/** `mq: Run selected text` — runs the current selection as an mq query against a chosen input file. */
class MqRunSelectedTextAction : AnAction() {

    override fun getActionUpdateThread(): ActionUpdateThread = ActionUpdateThread.BGT

    override fun update(e: AnActionEvent) {
        val editor = e.getData(CommonDataKeys.EDITOR)
        e.presentation.isEnabledAndVisible = editor != null && editor.selectionModel.hasSelection()
    }

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val editor = e.getData(CommonDataKeys.EDITOR) ?: return
        val query = editor.selectionModel.selectedText
        if (query.isNullOrBlank()) {
            Messages.showErrorDialog(project, "No text selected", "mq")
            return
        }

        pickInputFile(project) { inputFile ->
            val input = String(inputFile.contentsToByteArray(), inputFile.charset)
            MqCommandRunner.run(project, query, input, MqCommandRunner.inputFormatFor(inputFile))
        }
    }
}
