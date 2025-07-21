package org.mqlang.intellij.services

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.execution.process.ScriptRunnerUtil
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.components.Service
import com.intellij.openapi.project.Project
import com.intellij.openapi.ui.Messages
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.openapi.wm.ToolWindow
import com.intellij.openapi.wm.ToolWindowManager
import org.mqlang.intellij.settings.MqSettings
import org.mqlang.intellij.toolwindow.MqOutputToolWindowContent
import java.nio.charset.StandardCharsets

@Service(Service.Level.PROJECT)
class MqExecutionService(private val project: Project) {
    
    companion object {
        fun getInstance(project: Project): MqExecutionService {
            return project.getService(MqExecutionService::class.java)
        }
    }
    
    fun executeQuery(query: String, inputFile: VirtualFile) {
        ApplicationManager.getApplication().executeOnPooledThread {
            try {
                val settings = MqSettings.getInstance()
                val mqPath = settings.mqPath.ifEmpty { "mq" }
                
                val inputFormat = getInputFormat(inputFile.extension)
                
                val commandLine = GeneralCommandLine()
                    .withExePath(mqPath)
                    .withParameters("--input-format", inputFormat)
                    .withParameters("-")
                    .withInput(inputFile.inputStream)
                
                val processOutput = ScriptRunnerUtil.getProcessOutput(
                    commandLine.withInput(query.toByteArray(StandardCharsets.UTF_8)),
                    ScriptRunnerUtil.STDOUT_OUTPUT_KEY_FILTER,
                    60000 // 60 seconds timeout
                )
                
                ApplicationManager.getApplication().invokeLater {
                    if (processOutput.exitCode == 0) {
                        showOutput("mq Query Result", processOutput.stdout)
                    } else {
                        Messages.showErrorDialog(
                            project,
                            "Error: ${processOutput.stderr}",
                            "mq Execution Error"
                        )
                    }
                }
                
            } catch (e: Exception) {
                ApplicationManager.getApplication().invokeLater {
                    Messages.showErrorDialog(
                        project,
                        "Failed to execute mq: ${e.message}",
                        "Execution Error"
                    )
                }
            }
        }
    }
    
    fun executeMqFile(mqFile: VirtualFile, inputFile: VirtualFile) {
        ApplicationManager.getApplication().executeOnPooledThread {
            try {
                val settings = MqSettings.getInstance()
                val mqPath = settings.mqPath.ifEmpty { "mq" }
                
                val inputFormat = getInputFormat(inputFile.extension)
                val query = String(mqFile.contentsToByteArray(), StandardCharsets.UTF_8)
                
                val commandLine = GeneralCommandLine()
                    .withExePath(mqPath)
                    .withParameters("--input-format", inputFormat)
                    .withParameters("-")
                    .withInput(inputFile.inputStream)
                
                val processOutput = ScriptRunnerUtil.getProcessOutput(
                    commandLine.withInput(query.toByteArray(StandardCharsets.UTF_8)),
                    ScriptRunnerUtil.STDOUT_OUTPUT_KEY_FILTER,
                    60000 // 60 seconds timeout
                )
                
                ApplicationManager.getApplication().invokeLater {
                    if (processOutput.exitCode == 0) {
                        showOutput("mq File Result", processOutput.stdout)
                    } else {
                        Messages.showErrorDialog(
                            project,
                            "Error: ${processOutput.stderr}",
                            "mq Execution Error"
                        )
                    }
                }
                
            } catch (e: Exception) {
                ApplicationManager.getApplication().invokeLater {
                    Messages.showErrorDialog(
                        project,
                        "Failed to execute mq file: ${e.message}",
                        "Execution Error"
                    )
                }
            }
        }
    }
    
    private fun getInputFormat(extension: String?): String {
        return when (extension?.lowercase()) {
            "md" -> "markdown"
            "mdx" -> "mdx"
            "html" -> "html"
            "txt" -> "text"
            "csv" -> "csv"
            "tsv" -> "tsv"
            else -> "markdown"
        }
    }
    
    private fun showOutput(title: String, content: String) {
        val toolWindowManager = ToolWindowManager.getInstance(project)
        val toolWindow = toolWindowManager.getToolWindow("mq Output")
            ?: toolWindowManager.registerToolWindow("mq Output") {
                anchor = com.intellij.openapi.wm.ToolWindowAnchor.BOTTOM
                canCloseContent = false
            }
        
        val contentManager = toolWindow.contentManager
        contentManager.removeAllContents(true)
        
        val outputContent = MqOutputToolWindowContent(project, title, content)
        val content = contentManager.factory.createContent(outputContent.getComponent(), title, false)
        contentManager.addContent(content)
        contentManager.setSelectedContent(content)
        
        toolWindow.show()
    }
}