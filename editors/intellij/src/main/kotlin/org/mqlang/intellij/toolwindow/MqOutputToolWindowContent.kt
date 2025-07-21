package org.mqlang.intellij.toolwindow

import com.intellij.openapi.actionSystem.ActionManager
import com.intellij.openapi.actionSystem.ActionToolbar
import com.intellij.openapi.actionSystem.DefaultActionGroup
import com.intellij.openapi.editor.EditorFactory
import com.intellij.openapi.editor.ex.EditorEx
import com.intellij.openapi.fileTypes.PlainTextFileType
import com.intellij.openapi.ide.CopyPasteManager
import com.intellij.openapi.project.Project
import com.intellij.openapi.ui.SimpleToolWindowPanel
import com.intellij.ui.components.JBScrollPane
import java.awt.datatransfer.StringSelection
import javax.swing.JComponent
import javax.swing.JPanel

class MqOutputToolWindowContent(
    private val project: Project,
    private val title: String,
    private val content: String
) {
    
    private val mainPanel: JPanel
    
    init {
        mainPanel = createMainPanel()
    }
    
    fun getComponent(): JComponent = mainPanel
    
    private fun createMainPanel(): JPanel {
        val panel = SimpleToolWindowPanel(true, true)
        
        // Create toolbar
        val actionGroup = DefaultActionGroup().apply {
            add(CopyToClipboardAction())
        }
        val toolbar: ActionToolbar = ActionManager.getInstance()
            .createActionToolbar("MqOutputToolbar", actionGroup, true)
        panel.toolbar = toolbar.component
        
        // Create content editor
        val document = EditorFactory.getInstance().createDocument(content)
        val editor = EditorFactory.getInstance().createViewer(document, project, PlainTextFileType.INSTANCE) as EditorEx
        editor.settings.apply {
            isLineNumbersShown = false
            isLineMarkerAreaShown = false
            isFoldingOutlineShown = false
            isRightMarginShown = false
        }
        
        panel.setContent(JBScrollPane(editor.component))
        
        return panel
    }
    
    private inner class CopyToClipboardAction : com.intellij.openapi.actionSystem.AnAction(
        "Copy to Clipboard",
        "Copy the output to clipboard",
        com.intellij.icons.AllIcons.Actions.Copy
    ) {
        override fun actionPerformed(e: com.intellij.openapi.actionSystem.AnActionEvent) {
            CopyPasteManager.getInstance().setContents(StringSelection(content))
        }
    }
}