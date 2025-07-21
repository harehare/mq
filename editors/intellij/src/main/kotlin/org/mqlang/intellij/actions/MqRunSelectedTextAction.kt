package org.mqlang.intellij.actions

import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.actionSystem.CommonDataKeys
import com.intellij.openapi.editor.Editor
import com.intellij.openapi.fileChooser.FileChooser
import com.intellij.openapi.fileChooser.FileChooserDescriptorFactory
import com.intellij.openapi.project.Project
import com.intellij.openapi.ui.Messages
import com.intellij.openapi.vfs.VirtualFile
import org.mqlang.intellij.services.MqExecutionService

class MqRunSelectedTextAction : AnAction() {
    
    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val editor = e.getData(CommonDataKeys.EDITOR) ?: return
        
        val selectedText = getSelectedText(editor)
        if (selectedText.isNullOrEmpty()) {
            Messages.showErrorDialog(project, "No text selected", "Error")
            return
        }
        
        // Show file chooser for input file
        val descriptor = FileChooserDescriptorFactory.createSingleFileDescriptor()
            .withFileFilter { file -> 
                val ext = file.extension?.lowercase()
                ext in listOf("md", "mdx", "html", "txt", "csv", "tsv")
            }
        
        val selectedFile = FileChooser.chooseFile(descriptor, project, null)
        if (selectedFile != null) {
            executeQuery(project, selectedText, selectedFile)
        }
    }
    
    override fun update(e: AnActionEvent) {
        val editor = e.getData(CommonDataKeys.EDITOR)
        val hasSelection = editor?.selectionModel?.hasSelection() == true
        e.presentation.isEnabledAndVisible = hasSelection
    }
    
    private fun getSelectedText(editor: Editor): String? {
        val selectionModel = editor.selectionModel
        return if (selectionModel.hasSelection()) {
            selectionModel.selectedText
        } else null
    }
    
    private fun executeQuery(project: Project, query: String, inputFile: VirtualFile) {
        MqExecutionService.getInstance(project).executeQuery(query, inputFile)
    }
}