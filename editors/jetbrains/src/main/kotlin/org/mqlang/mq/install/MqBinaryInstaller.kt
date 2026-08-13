package org.mqlang.mq.install

import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.progress.ProgressIndicator
import com.intellij.openapi.progress.ProgressManager
import com.intellij.openapi.progress.Task
import com.intellij.openapi.project.Project
import java.io.IOException
import java.net.URI
import java.net.http.HttpClient
import java.net.http.HttpRequest
import java.net.http.HttpResponse
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.StandardCopyOption
import java.time.Duration
import org.mqlang.mq.MqPluginInfo

/** Downloads prebuilt `mq-lsp` / `mq-dbg` binaries from the mq GitHub Releases, mirroring the VS Code extension installer. */
object MqBinaryInstaller {

    private const val RELEASE_BASE_URL = "https://github.com/harehare/mq/releases/download"

    private fun targetTriple(): String? {
        val os = System.getProperty("os.name").lowercase()
        val arch = System.getProperty("os.arch").lowercase()
        return when {
            os.contains("mac") && (arch.contains("aarch64") || arch.contains("arm")) -> "aarch64-apple-darwin"
            os.contains("linux") && arch.contains("aarch64") -> "aarch64-unknown-linux-gnu"
            os.contains("linux") -> "x86_64-unknown-linux-gnu"
            os.contains("win") -> "x86_64-pc-windows-msvc"
            else -> null
        }
    }

    /** Downloads mq-lsp and mq-dbg into the plugin's data directory, showing progress in [project]. Invokes [onFinished] on the EDT with success/failure. */
    fun installInBackground(project: Project?, onFinished: (Boolean) -> Unit) {
        ProgressManager.getInstance().run(object : Task.Backgroundable(project, "mq: Downloading binaries from GitHub Releases", true) {
            override fun run(indicator: ProgressIndicator) {
                val success = try {
                    install(indicator)
                    true
                } catch (e: Exception) {
                    notifyError(project, "mq: Download failed: ${e.message}")
                    false
                }
                onFinished(success)
            }
        })
    }

    private fun install(indicator: ProgressIndicator) {
        val target = targetTriple() ?: throw IOException(
            "No prebuilt binary available for ${System.getProperty("os.name")}/${System.getProperty("os.arch")}. " +
                "Please install mq-lsp and mq-dbg manually (e.g. via cargo install) and set their paths in Settings > mq.",
        )
        val ext = if (MqBinaryLocator.isWindows()) ".exe" else ""
        val dir = MqBinaryLocator.binariesDir()
        Files.createDirectories(dir)

        val version = MqPluginInfo.version
        indicator.text = "Downloading mq-lsp..."
        indicator.isIndeterminate = true
        downloadFile(
            URI.create("$RELEASE_BASE_URL/v$version/mq-lsp-$target$ext"),
            MqBinaryLocator.downloadedLspPath(),
        )

        indicator.text = "Downloading mq-dbg..."
        downloadFile(
            URI.create("$RELEASE_BASE_URL/v$version/mq-dbg-$target$ext"),
            MqBinaryLocator.downloadedDbgPath(),
        )

        if (!MqBinaryLocator.isWindows()) {
            makeExecutable(MqBinaryLocator.downloadedLspPath())
            makeExecutable(MqBinaryLocator.downloadedDbgPath())
        }
    }

    private fun downloadFile(uri: URI, dest: Path) {
        val client = HttpClient.newBuilder()
            .followRedirects(HttpClient.Redirect.NORMAL)
            .connectTimeout(Duration.ofSeconds(30))
            .build()
        val request = HttpRequest.newBuilder(uri).GET().build()
        val response = client.send(request, HttpResponse.BodyHandlers.ofInputStream())
        if (response.statusCode() != 200) {
            throw IOException("Download failed with status ${response.statusCode()} for $uri")
        }
        response.body().use { input ->
            Files.copy(input, dest, StandardCopyOption.REPLACE_EXISTING)
        }
    }

    private fun makeExecutable(path: Path) {
        val file = path.toFile()
        file.setExecutable(true, false)
        file.setReadable(true, false)
    }

    private fun notifyError(project: Project?, message: String) {
        NotificationGroupManager.getInstance()
            .getNotificationGroup("mq")
            .createNotification(message, NotificationType.ERROR)
            .notify(project)
    }
}
