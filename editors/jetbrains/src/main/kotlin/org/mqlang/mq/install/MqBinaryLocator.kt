package org.mqlang.mq.install

import com.intellij.execution.configurations.PathEnvironmentVariableUtil
import com.intellij.openapi.application.PathManager
import org.mqlang.mq.settings.MqSettingsState
import java.io.File
import java.nio.file.Path

/** Locates the `mq-lsp` / `mq-dbg` executables: explicit setting > downloaded copy > PATH. */
object MqBinaryLocator {

    private const val EXE_SUFFIX_WINDOWS = ".exe"

    fun binariesDir(): Path = Path.of(PathManager.getSystemPath(), "mq", "bin")

    private fun exeName(base: String): String =
        if (isWindows()) "$base$EXE_SUFFIX_WINDOWS" else base

    fun isWindows(): Boolean = System.getProperty("os.name").lowercase().contains("win")

    fun downloadedLspPath(): Path = binariesDir().resolve(exeName("mq-lsp"))
    fun downloadedDbgPath(): Path = binariesDir().resolve(exeName("mq-dbg"))

    /** Resolves the mq-lsp executable path, or null if it cannot be found anywhere. */
    fun findLspPath(): String? {
        val settings = MqSettingsState.getInstance()
        if (settings.lspPath.isNotBlank()) {
            return settings.lspPath
        }
        val downloaded = downloadedLspPath()
        if (downloaded.toFile().canExecute()) {
            return downloaded.toString()
        }
        return findOnPath("mq-lsp")
    }

    /** Resolves the mq-dbg executable path, or null if it cannot be found anywhere. */
    fun findDbgPath(): String? {
        val settings = MqSettingsState.getInstance()
        if (settings.dbgPath.isNotBlank()) {
            return settings.dbgPath
        }
        val downloaded = downloadedDbgPath()
        if (downloaded.toFile().canExecute()) {
            return downloaded.toString()
        }
        return findOnPath("mq-dbg")
    }

    private fun findOnPath(binaryName: String): String? {
        val exe = exeName(binaryName)
        val found: File? = PathEnvironmentVariableUtil.findInPath(exe)
        return found?.absolutePath
    }
}
