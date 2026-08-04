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
 * `.mq` files get syntax highlighting without a hand-written Lexer. The grammar's own `fileTypes: ["mq"]`
 * entry is what IntelliJ's TextMate bundle loader uses to associate the `.mq` extension with this scope.
 *
 * Bundle resources live inside the plugin jar, so they can't be referenced as a filesystem [Path] directly;
 * they're extracted once into the IDE system directory instead.
 */
class MqTextMateBundleProvider : TextMateBundleProvider {

    override fun getBundles(): List<TextMateBundleProvider.PluginBundle> =
        listOf(TextMateBundleProvider.PluginBundle("mq", extractBundle()))

    private fun extractBundle(): Path {
        val bundleDir = Path.of(PathManager.getSystemPath(), "mq", "textmate-bundle-${MqPluginInfo.version}")
        val grammarDest = bundleDir.resolve("Syntaxes").resolve("mq.tmLanguage.json")

        if (Files.exists(grammarDest)) {
            return bundleDir
        }

        try {
            Files.createDirectories(grammarDest.parent)
            javaClass.classLoader.getResourceAsStream("textmate/Syntaxes/mq.tmLanguage.json")?.use { input ->
                Files.copy(input, grammarDest, StandardCopyOption.REPLACE_EXISTING)
            }
        } catch (e: Exception) {
            logger<MqTextMateBundleProvider>().warn("Failed to extract mq TextMate bundle", e)
        }

        return bundleDir
    }
}
