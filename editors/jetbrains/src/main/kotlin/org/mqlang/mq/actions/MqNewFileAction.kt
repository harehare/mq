package org.mqlang.mq.actions

import com.intellij.openapi.actionSystem.ActionUpdateThread
import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.fileEditor.FileEditorManager
import com.intellij.testFramework.LightVirtualFile
import org.mqlang.mq.MqExamples
import org.mqlang.mq.settings.MqSettingsState

/** `mq: New File` — mirrors the VS Code extension's "mq.new" command. */
class MqNewFileAction : AnAction() {

    override fun getActionUpdateThread(): ActionUpdateThread = ActionUpdateThread.BGT

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val content = if (MqSettingsState.getInstance().showExamplesInNewFile) MqExamples.TEXT else ""
        val file = LightVirtualFile("untitled.mq", content)
        FileEditorManager.getInstance(project).openFile(file, true)
    }
}
