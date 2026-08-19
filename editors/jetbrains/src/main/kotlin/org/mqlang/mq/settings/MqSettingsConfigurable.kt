package org.mqlang.mq.settings

import com.intellij.openapi.options.Configurable
import com.intellij.openapi.ui.TextBrowseFolderListener
import com.intellij.openapi.ui.TextFieldWithBrowseButton
import com.intellij.openapi.fileChooser.FileChooserDescriptorFactory
import com.intellij.ui.components.JBCheckBox
import com.intellij.ui.components.JBTextField
import com.intellij.util.ui.FormBuilder
import javax.swing.JComponent
import javax.swing.JPanel

/** Settings > Languages & Frameworks > mq */
class MqSettingsConfigurable : Configurable {

    private val settings get() = MqSettingsState.getInstance()

    private val lspPathField = TextFieldWithBrowseButton().apply {
        addBrowseFolderListener(
            TextBrowseFolderListener(
                FileChooserDescriptorFactory.createSingleFileNoJarsDescriptor()
                    .withTitle("mq-lsp Executable")
                    .withDescription("Path to the mq-lsp language server executable. Leave empty to auto-detect on PATH."),
            ),
        )
    }
    private val dbgPathField = TextFieldWithBrowseButton().apply {
        addBrowseFolderListener(
            TextBrowseFolderListener(
                FileChooserDescriptorFactory.createSingleFileNoJarsDescriptor()
                    .withTitle("mq-dbg Executable")
                    .withDescription("Path to the mq-dbg debug adapter executable. Leave empty to auto-detect on PATH."),
            ),
        )
    }
    private val showExamplesCheckBox = JBCheckBox("Show examples when creating a new .mq file")
    private val enableTypeCheckCheckBox = JBCheckBox("Enable type checking (passes --enable-type-checking to mq-lsp)")
    private val strictArrayCheckBox =
        JBCheckBox("Strict array mode (passes --strict-array to mq-lsp, requires type checking)")
    private val enableLintCheckBox = JBCheckBox("Enable mq-lint diagnostics (passes --enable-lint to mq-lsp)")
    private val disabledLintRulesField = JBTextField()

    private var panel: JPanel? = null

    override fun getDisplayName(): String = "mq"

    override fun createComponent(): JComponent {
        val built = FormBuilder.createFormBuilder()
            .addLabeledComponent("mq-lsp path:", lspPathField)
            .addLabeledComponent("mq-dbg path:", dbgPathField)
            .addComponent(showExamplesCheckBox)
            .addSeparator()
            .addComponent(enableTypeCheckCheckBox)
            .addComponent(strictArrayCheckBox)
            .addSeparator()
            .addComponent(enableLintCheckBox)
            .addLabeledComponent("Disabled lint rules (comma separated):", disabledLintRulesField)
            .addComponentFillVertically(JPanel(), 0)
            .panel
        panel = built
        return built
    }

    override fun isModified(): Boolean {
        val s = settings
        return lspPathField.text != s.lspPath ||
            dbgPathField.text != s.dbgPath ||
            showExamplesCheckBox.isSelected != s.showExamplesInNewFile ||
            enableTypeCheckCheckBox.isSelected != s.enableTypeCheck ||
            strictArrayCheckBox.isSelected != s.strictArray ||
            enableLintCheckBox.isSelected != s.enableLint ||
            disabledLintRulesField.text.trim() != s.disabledLintRules.joinToString(", ")
    }

    override fun apply() {
        val s = settings
        s.lspPath = lspPathField.text.trim()
        s.dbgPath = dbgPathField.text.trim()
        s.showExamplesInNewFile = showExamplesCheckBox.isSelected
        s.enableTypeCheck = enableTypeCheckCheckBox.isSelected
        s.strictArray = strictArrayCheckBox.isSelected
        s.enableLint = enableLintCheckBox.isSelected
        s.disabledLintRules = disabledLintRulesField.text
            .split(",")
            .map { it.trim() }
            .filter { it.isNotEmpty() }
    }

    override fun reset() {
        val s = settings
        lspPathField.text = s.lspPath
        dbgPathField.text = s.dbgPath
        showExamplesCheckBox.isSelected = s.showExamplesInNewFile
        enableTypeCheckCheckBox.isSelected = s.enableTypeCheck
        strictArrayCheckBox.isSelected = s.strictArray
        enableLintCheckBox.isSelected = s.enableLint
        disabledLintRulesField.text = s.disabledLintRules.joinToString(", ")
    }

    override fun disposeUIResources() {
        panel = null
    }
}
