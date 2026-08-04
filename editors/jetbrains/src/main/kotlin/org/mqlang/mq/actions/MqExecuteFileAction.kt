package org.mqlang.mq.actions

import com.intellij.openapi.actionSystem.ActionUpdateThread
import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.actionSystem.CommonDataKeys
import com.intellij.openapi.fileEditor.FileDocumentManager

/** `mq: Execute mq file` — runs a chosen `.mq` file's content against the active editor's text. */
class MqExecuteFileAction : AnAction() {

    override fun getActionUpdateThread(): ActionUpdateThread = ActionUpdateThread.BGT

    override fun update(e: AnActionEvent) {
        e.presentation.isEnabledAndVisible = e.getData(CommonDataKeys.EDITOR) != null
    }

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val editor = e.getData(CommonDataKeys.EDITOR) ?: return
        val activeFile = FileDocumentManager.getInstance().getFile(editor.document) ?: return

        pickMqFile(project) { mqFile ->
            val query = String(mqFile.contentsToByteArray(), mqFile.charset)
            MqCommandRunner.run(project, query, editor.document.text, MqCommandRunner.inputFormatFor(activeFile))
        }
    }
}
