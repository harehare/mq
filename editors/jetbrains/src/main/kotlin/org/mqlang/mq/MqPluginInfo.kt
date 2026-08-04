package org.mqlang.mq

import com.intellij.ide.plugins.PluginManagerCore
import com.intellij.openapi.extensions.PluginId

object MqPluginInfo {
    val pluginId: PluginId = PluginId.getId("org.mqlang.mq")

    /** The mq release version this plugin build targets (matches mq-lsp/mq-dbg GitHub Release assets). */
    val version: String
        get() = PluginManagerCore.getPlugin(pluginId)?.version ?: "0.7.0"
}
