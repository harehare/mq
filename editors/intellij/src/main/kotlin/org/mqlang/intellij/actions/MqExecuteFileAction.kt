package org.mqlang.intellij.actions

import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.actionSystem.CommonDataKeys
import com.intellij.openapi.fileChooser.FileChooser
import com.intellij.openapi.fileChooser.FileChooserDescriptorFactory
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import org.mqlang.intellij.services.MqExecutionService

class MqExecuteFileAction : AnAction() {
    
    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val currentFile = e.getData(CommonDataKeys.VIRTUAL_FILE) ?: return
        
        // Show file chooser for mq file
        val descriptor = FileChooserDescriptorFactory.createSingleFileDescriptor()
            .withFileFilter { file -> file.extension?.lowercase() == "mq" }
        
        val mqFile = FileChooser.chooseFile(descriptor, project, null)
        if (mqFile != null) {
            executeMqFile(project, mqFile, currentFile)
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
    
    private fun executeMqFile(project: Project, mqFile: VirtualFile, inputFile: VirtualFile) {
        MqExecutionService.getInstance(project).executeMqFile(mqFile, inputFile)
    }
}