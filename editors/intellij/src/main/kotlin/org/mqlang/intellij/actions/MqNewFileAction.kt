package org.mqlang.intellij.actions

import com.intellij.ide.actions.CreateFileFromTemplateAction
import com.intellij.ide.actions.CreateFileFromTemplateDialog
import com.intellij.openapi.project.DumbAware
import com.intellij.openapi.project.Project
import com.intellij.psi.PsiDirectory
import org.mqlang.intellij.MqIcons

class MqNewFileAction : CreateFileFromTemplateAction("mq File", "Create new mq file", MqIcons.FILE), DumbAware {
    
    override fun buildDialog(project: Project, directory: PsiDirectory, builder: CreateFileFromTemplateDialog.Builder) {
        builder
            .setTitle("New mq File")
            .addKind("Empty file", MqIcons.FILE, "mq File")
            .addKind("With examples", MqIcons.FILE, "mq File with Examples")
    }
    
    override fun getActionName(directory: PsiDirectory?, newName: String, templateName: String?): String {
        return "Create mq File"
    }
}