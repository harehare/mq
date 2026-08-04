package org.mqlang.mq

import com.intellij.ide.plugins.cl.PluginAwareClassLoader

object MqPluginInfo {
    /** The plugin's id, as declared in `plugin.xml`. */
    const val PLUGIN_ID = "org.mqlang.mq"

    /** Used when the descriptor cannot be resolved, e.g. when running outside a plugin classloader. */
    private const val FALLBACK_VERSION = "0.7.0"

    /**
     * The mq release version this plugin build targets (matches mq-lsp/mq-dbg GitHub Release assets).
     *
     * Read off the plugin's own classloader rather than via `PluginManagerCore.getPlugin(PluginId.getId(id))`:
     * `PluginId` became a Kotlin class in the 2025.x platform, so Kotlin code compiled against it emits a
     * `PluginId.Companion` reference that does not exist on the older platforms this plugin still declares
     * support for, failing with `NoSuchFieldError` at class initialization. Only instance members are used
     * here, which stay binary compatible across platform versions.
     */
    val version: String
        get() = (javaClass.classLoader as? PluginAwareClassLoader)?.pluginDescriptor?.version
            ?: FALLBACK_VERSION
}
