package org.mqlang.intellij.actions

import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.actionSystem.CommonDataKeys
import com.intellij.openapi.project.Project
import com.intellij.openapi.ui.Messages
import com.intellij.openapi.vfs.VirtualFile
import org.mqlang.intellij.services.MqExecutionService

class MqExecuteQueryAction : AnAction() {
    
    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val file = e.getData(CommonDataKeys.VIRTUAL_FILE) ?: return
        
        val query = Messages.showInputDialog(
            project,
            "Enter mq query to execute:",
            "Execute mq Query",
            null,
            ".[] | upcase()",
            null
        )
        
        if (!query.isNullOrEmpty()) {
            executeQuery(project, query, file)
        }
    }
    
    override fun update(e: AnActionEvent) {
        val file = e.getData(CommonDataKeys.VIRTUAL_FILE)
        val isMarkdownFile = file?.let { 
            val ext = it.extension?.lowercase()
            ext in listOf("md", "mdx", "html", "txt", "csv", "tsv")
        } == true
        e.presentation.isEnabledAndVisible = isMarkdownFile
    }
    
    private fun executeQuery(project: Project, query: String, inputFile: VirtualFile) {
        MqExecutionService.getInstance(project).executeQuery(query, inputFile)
    }
}