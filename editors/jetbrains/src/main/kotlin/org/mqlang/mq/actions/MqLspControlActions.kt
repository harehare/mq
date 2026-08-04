package org.mqlang.mq.actions

import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.actionSystem.ActionUpdateThread
import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.redhat.devtools.lsp4ij.LanguageServerManager
import org.mqlang.mq.install.MqBinaryInstaller
import org.mqlang.mq.install.MqBinaryLocator

private const val MQ_SERVER_ID = "mq"

/** `mq: Start LSP Server` */
class MqStartLspServerAction : AnAction() {
    override fun getActionUpdateThread(): ActionUpdateThread = ActionUpdateThread.BGT
    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        LanguageServerManager.getInstance(project).start(MQ_SERVER_ID)
    }
}

/** `mq: Stop LSP Server` */
class MqStopLspServerAction : AnAction() {
    override fun getActionUpdateThread(): ActionUpdateThread = ActionUpdateThread.BGT
    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        LanguageServerManager.getInstance(project).stop(MQ_SERVER_ID)
    }
}

/** `mq: Restart LSP Server` */
class MqRestartLspServerAction : AnAction() {
    override fun getActionUpdateThread(): ActionUpdateThread = ActionUpdateThread.BGT
    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val manager = LanguageServerManager.getInstance(project)
        manager.stop(MQ_SERVER_ID)
        manager.start(MQ_SERVER_ID)
    }
}

/** `mq: Install Servers` — downloads mq-lsp/mq-dbg from GitHub Releases, then (re)starts the LSP server. */
class MqInstallServersAction : AnAction() {
    override fun getActionUpdateThread(): ActionUpdateThread = ActionUpdateThread.BGT
    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project
        val manager = project?.let { LanguageServerManager.getInstance(it) }
        manager?.stop(MQ_SERVER_ID)

        MqBinaryInstaller.installInBackground(project) { success ->
            if (success) {
                NotificationGroupManager.getInstance()
                    .getNotificationGroup("mq")
                    .createNotification(
                        "mq: Binaries installed to ${MqBinaryLocator.binariesDir()}",
                        NotificationType.INFORMATION,
                    )
                    .notify(project)
                manager?.start(MQ_SERVER_ID)
            }
        }
    }
}
