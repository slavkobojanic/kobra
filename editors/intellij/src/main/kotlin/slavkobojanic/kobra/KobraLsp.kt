package slavkobojanic.kobra

import com.intellij.execution.ExecutionException
import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.notification.Notification
import com.intellij.notification.NotificationAction
import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.options.ShowSettingsUtil
import com.intellij.openapi.project.DumbAwareAction
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.Key
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.platform.lsp.api.LspClientManager
import com.intellij.platform.lsp.api.LspIntegrationProvider
import com.intellij.platform.lsp.api.ProjectWideLspClientDescriptor
import java.nio.charset.StandardCharsets
import java.nio.file.Path

/** Starts `k lsp` for the project when a Kobra file opens. */
class KobraLspIntegrationProvider : LspIntegrationProvider {
    override fun fileOpened(
        project: Project,
        file: VirtualFile,
        clientStarter: LspIntegrationProvider.LspClientStarter,
    ) {
        if (!isKobra(file)) return
        if (KobraToolchain.find(KobraSettings.getInstance().serverPath) == null) {
            notifyMissing(project)
            return
        }
        clientStarter.ensureClientStarted(KobraLspClientDescriptor(project))
    }

    companion object {
        private val NOTIFIED = Key.create<Boolean>("kobra.missing.k.notified")

        /** Restarts the project's server, after the settings change or a new `k` is installed. */
        fun restart(project: Project) {
            project.putUserData(NOTIFIED, null)
            LspClientManager.getInstance(project)
                .stopAndRestartClientsIfNeeded(KobraLspIntegrationProvider::class.java)
        }

        /** Once per project until the settings change: `k` isn't found, and how to install it. */
        fun notifyMissing(project: Project) {
            if (project.getUserData(NOTIFIED) == true) return
            project.putUserData(NOTIFIED, true)
            val configured = KobraSettings.getInstance().serverPath
            val what = if (configured.isEmpty()) {
                "<code>k</code> isn't on <code>PATH</code> or in <code>~/.k/bin</code>"
            } else {
                "<code>${escape(configured)}</code> (Settings | Languages &amp; Frameworks | Kobra) isn't an executable"
            }
            NotificationGroupManager.getInstance()
                .getNotificationGroup("Kobra")
                .createNotification(
                    "Kobra: no language server",
                    "$what, so Kobra files have highlighting only. Install Kobra with the GitHub CLI " +
                        "(<code>gh auth login</code> first):<br><code>${escape(KobraToolchain.INSTALL_SH)}</code><br>" +
                        "or on Windows:<br><code>${escape(KobraToolchain.INSTALL_PS1)}</code><br>" +
                        "then restart the server (Tools | Restart Kobra Language Server).",
                    NotificationType.WARNING,
                )
                .addAction(NotificationAction.createSimple("Settings…") {
                    ShowSettingsUtil.getInstance().showSettingsDialog(project, KobraConfigurable::class.java)
                })
                .addAction(object : NotificationAction("Restart server") {
                    override fun actionPerformed(e: AnActionEvent, notification: Notification) {
                        notification.expire()
                        restart(project)
                    }
                })
                .notify(project)
        }

        private fun escape(text: String) = text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
    }
}

internal fun isKobra(file: VirtualFile): Boolean =
    file.fileType == KobraFileType.INSTANCE || file.extension == KobraFileType.EXTENSION

/** `k lsp` over standard input and output, for the project's Kobra files, run in the project's directory. */
class KobraLspClientDescriptor(project: Project) : ProjectWideLspClientDescriptor(project, "Kobra") {
    override fun isSupportedFile(file: VirtualFile): Boolean = isKobra(file)

    /** The VS Code extension's language id. */
    override fun getLanguageId(file: VirtualFile): String = "k"

    override fun createCommandLine(): GeneralCommandLine {
        val k = KobraToolchain.find(KobraSettings.getInstance().serverPath)
            ?: throw ExecutionException("Kobra: `k` wasn't found. Install it with: ${KobraToolchain.INSTALL_SH}")
        return commandLine(k, project.basePath)
    }

    companion object {
        fun commandLine(k: Path, directory: String?): GeneralCommandLine =
            GeneralCommandLine(k.toString(), "lsp")
                .withCharset(StandardCharsets.UTF_8)
                .withParentEnvironmentType(GeneralCommandLine.ParentEnvironmentType.CONSOLE)
                .apply { if (directory != null) withWorkDirectory(directory) }
    }
}

/** Tools | Restart Kobra Language Server, like the VS Code command. */
class KobraRestartServerAction : DumbAwareAction() {
    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        KobraLspIntegrationProvider.restart(project)
        if (KobraToolchain.find(KobraSettings.getInstance().serverPath) == null) {
            KobraLspIntegrationProvider.notifyMissing(project)
        }
    }
}
