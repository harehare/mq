package org.mqlang.intellij.lsp

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.platform.lsp.api.LspServerSupportProvider
import com.intellij.platform.lsp.api.ProjectWideLspServerDescriptor
import org.mqlang.intellij.MqFileType
import org.mqlang.intellij.settings.MqSettings

class MqLspServerDefinition : ProjectWideLspServerDescriptor(project, "mq") {
    
    override fun isSupportedFile(file: VirtualFile): Boolean {
        return file.fileType == MqFileType
    }
    
    override fun createCommandLine(): GeneralCommandLine {
        val settings = MqSettings.getInstance()
        val mqPath = settings.mqPath.ifEmpty { "mq" }
        
        return GeneralCommandLine().apply {
            exePath = mqPath
            addParameter("lsp")
        }
    }
    
    private val project: Project
        get() = super.project
}

class MqLspServerSupportProvider : LspServerSupportProvider {
    override fun fileOpened(project: Project, file: VirtualFile, serverStarter: LspServerSupportProvider.LspServerStarter) {
        if (file.fileType == MqFileType) {
            val settings = MqSettings.getInstance()
            if (settings.enableLsp) {
                serverStarter.ensureServerStarted(MqLspServerDefinition(project))
            }
        }
    }
}