package org.mqlang.mq.dap

import com.intellij.execution.ExecutionException
import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.execution.configurations.RunConfigurationOptions
import com.intellij.execution.process.ProcessHandler
import com.intellij.execution.runners.ExecutionEnvironment
import com.intellij.openapi.fileTypes.FileType
import com.redhat.devtools.lsp4ij.dap.client.LaunchUtils
import com.redhat.devtools.lsp4ij.dap.configurations.DAPRunConfigurationOptions
import com.redhat.devtools.lsp4ij.dap.definitions.DebugAdapterServerDefinition
import com.redhat.devtools.lsp4ij.dap.descriptors.DebugAdapterDescriptor
import org.mqlang.mq.install.MqBinaryLocator

/**
 * Starts `mq-dbg dap` directly (rather than relying on a user-typed command in the generic DAP run configuration
 * UI), so the "Debug current file" action and the auto-created run configuration both just work once mq-dbg is
 * installed/located, mirroring how the mq VS Code extension wires up its `mq` debugger type.
 */
class MqDebugAdapterDescriptor(
    options: RunConfigurationOptions,
    environment: ExecutionEnvironment,
    serverDefinition: DebugAdapterServerDefinition?,
) : DebugAdapterDescriptor(options, environment, serverDefinition) {

    override fun startServer(): ProcessHandler {
        val dbgPath = MqBinaryLocator.findDbgPath()
            ?: throw ExecutionException(
                "mq-dbg not found. Install it via the \"mq: Install Servers\" action or set its path in Settings > mq.",
            )
        return startServer(GeneralCommandLine(dbgPath, "dap"))
    }

    override fun getDapParameters(): Map<String, Any> {
        val opts = options as? DAPRunConfigurationOptions ?: return emptyMap()
        val context = mapOf("\${file}" to (opts.file.orEmpty()))
        return LaunchUtils.getDapParameters(opts.launchConfiguration, context)
    }

    override fun getFileType(): FileType? = null
}
