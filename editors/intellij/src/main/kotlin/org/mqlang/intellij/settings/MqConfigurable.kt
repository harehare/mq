package org.mqlang.intellij.settings

import com.intellij.openapi.options.Configurable
import com.intellij.openapi.ui.DialogPanel
import com.intellij.ui.dsl.builder.panel
import javax.swing.JComponent

class MqConfigurable : Configurable {
    
    private val settings = MqSettings.getInstance()
    private lateinit var mainPanel: DialogPanel
    
    override fun getDisplayName(): String = "mq"
    
    override fun createComponent(): JComponent {
        mainPanel = panel {
            group("mq Configuration") {
                row("mq executable path:") {
                    textField()
                        .bindText(settings::mqPath)
                        .comment("Path to the mq executable. Leave empty to use PATH.")
                }
                row {
                    checkBox("Show examples in new file")
                        .bindSelected(settings::showExamplesInNewFile)
                        .comment("Show example queries when creating a new mq file")
                }
                row {
                    checkBox("Enable LSP")
                        .bindSelected(settings::enableLsp)
                        .comment("Enable Language Server Protocol support for better code completion and validation")
                }
            }
        }
        return mainPanel
    }
    
    override fun isModified(): Boolean = mainPanel.isModified()
    
    override fun apply() {
        mainPanel.apply()
    }
    
    override fun reset() {
        mainPanel.reset()
    }
}