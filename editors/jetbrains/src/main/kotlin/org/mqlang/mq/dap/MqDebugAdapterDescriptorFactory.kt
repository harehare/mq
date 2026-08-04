package org.mqlang.mq.dap

import com.intellij.execution.configurations.RunConfigurationOptions
import com.intellij.execution.runners.ExecutionEnvironment
import com.redhat.devtools.lsp4ij.dap.DebugMode
import com.redhat.devtools.lsp4ij.dap.LaunchConfiguration
import com.redhat.devtools.lsp4ij.dap.descriptors.DebugAdapterDescriptor
import com.redhat.devtools.lsp4ij.dap.descriptors.DebugAdapterDescriptorFactory

/** Registers `mq-dbg` as an LSP4IJ Debug Adapter Protocol server (id "mq"), reusing the generic DAP run configuration UI. */
class MqDebugAdapterDescriptorFactory : DebugAdapterDescriptorFactory() {

    override fun createDebugAdapterDescriptor(
        options: RunConfigurationOptions,
        environment: ExecutionEnvironment,
    ): DebugAdapterDescriptor = MqDebugAdapterDescriptor(options, environment, serverDefinition)

    override fun getLaunchConfigurations(): List<LaunchConfiguration> = listOf(
        LaunchConfiguration(
            "mq_launch",
            "Debug mq query",
            // language=json
            """
            {
              "type": "mq",
              "name": "Debug mq query",
              "request": "launch",
              "queryFile": "${'$'}{file}",
              "inputFile": ""
            }
            """.trimIndent(),
            DebugMode.LAUNCH,
        ),
    )
}
