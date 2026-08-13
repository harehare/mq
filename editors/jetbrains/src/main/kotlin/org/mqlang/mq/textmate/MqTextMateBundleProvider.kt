package org.mqlang.mq.textmate

import com.intellij.openapi.application.PathManager
import com.intellij.openapi.diagnostic.logger
import org.jetbrains.plugins.textmate.api.TextMateBundleProvider
import org.mqlang.mq.MqPluginInfo
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.StandardCopyOption

/**
 * Registers the mq TextMate grammar (converted from the mq VS Code extension's `mq.tmLanguage.json`) so
 * `.mq` files get syntax highlighting without a hand-written Lexer.
 *
 * Bundle resources live inside the plugin jar, so they can't be referenced as a filesystem [Path] directly;
 * they're extracted once into the IDE system directory instead. The extracted directory must look like a
 * VS Code extension — IntelliJ's bundle loader detects the bundle type by looking for a `package.json`
 * (VS Code), `info.plist` (TextMate) or `*.tmBundle` file, and rejects the bundle outright when none is
 * present. `package.json` is therefore what maps the `source.mq` scope and the `.mq` extension to the
 * grammar; the grammar's own `fileTypes` entry alone is not enough.
 */
class MqTextMateBundleProvider : TextMateBundleProvider {

    override fun getBundles(): List<TextMateBundleProvider.PluginBundle> =
        listOfNotNull(extractBundle()?.let { TextMateBundleProvider.PluginBundle("mq", it) })

    /** Extracts the bundle into the IDE system directory, returning null if it could not be written. */
    private fun extractBundle(): Path? {
        val bundleDir = Path.of(PathManager.getSystemPath(), "mq", "textmate-bundle-${MqPluginInfo.version}")

        if (BUNDLE_RESOURCES.all { Files.exists(bundleDir.resolve(it)) }) {
            return bundleDir
        }

        return try {
            for (resource in BUNDLE_RESOURCES) {
                val dest = bundleDir.resolve(resource)
                Files.createDirectories(dest.parent)
                val stream = javaClass.classLoader.getResourceAsStream("textmate/$resource")
                    ?: error("missing bundled resource: textmate/$resource")
                stream.use { Files.copy(it, dest, StandardCopyOption.REPLACE_EXISTING) }
            }
            bundleDir
        } catch (e: Exception) {
            logger<MqTextMateBundleProvider>().warn("Failed to extract mq TextMate bundle", e)
            null
        }
    }

    companion object {
        /** Bundle-relative paths of the resources making up the VS Code style TextMate bundle. */
        private val BUNDLE_RESOURCES = listOf(
            "package.json",
            "language-configuration.json",
            "Syntaxes/mq.tmLanguage.json",
        )
    }
}
