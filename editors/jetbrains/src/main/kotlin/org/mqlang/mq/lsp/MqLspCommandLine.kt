package org.mqlang.mq.lsp

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.openapi.project.Project
import org.mqlang.mq.install.MqBinaryLocator
import org.mqlang.mq.settings.MqSettingsState

/** Builds the `mq-lsp` command line from plugin settings, mirroring the mq VS Code extension's server args. */
object MqLspCommandLine {

    /** @throws IllegalStateException if mq-lsp cannot be located. */
    fun build(project: Project): GeneralCommandLine {
        val lspPath = MqBinaryLocator.findLspPath()
            ?: error("mq-lsp not found. Install it via the \"mq: Install Servers\" action or set its path in Settings > mq.")

        val settings = MqSettingsState.getInstance()
        val commandLine = GeneralCommandLine(lspPath)

        project.basePath?.let { basePath ->
            commandLine.addParameters("-M", basePath)
        }

        if (settings.enableTypeCheck) {
            commandLine.addParameter("--enable-type-checking")
            if (settings.strictArray) {
                commandLine.addParameter("--strict-array")
            }
        }

        if (settings.enableLint) {
            commandLine.addParameter("--enable-lint")
            settings.disabledLintRules.forEach { ruleId ->
                commandLine.addParameters("--disable-lint-rule", ruleId)
            }
        }

        return commandLine
    }
}
