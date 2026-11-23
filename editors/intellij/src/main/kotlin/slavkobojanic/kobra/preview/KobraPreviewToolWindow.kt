package slavkobojanic.kobra.preview

import com.intellij.icons.AllIcons
import com.intellij.ide.BrowserUtil
import com.intellij.ide.ui.LafManagerListener
import com.intellij.openapi.Disposable
import com.intellij.openapi.actionSystem.ActionManager
import com.intellij.openapi.actionSystem.ActionUpdateThread
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.actionSystem.DefaultActionGroup
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.project.DumbAware
import com.intellij.openapi.project.DumbAwareAction
import com.intellij.openapi.project.DumbAwareToggleAction
import com.intellij.openapi.project.Project
import com.intellij.openapi.startup.ProjectActivity
import com.intellij.openapi.ui.SimpleToolWindowPanel
import com.intellij.openapi.util.Disposer
import com.intellij.openapi.wm.ToolWindow
import com.intellij.openapi.wm.ToolWindowFactory
import com.intellij.ui.HyperlinkLabel
import com.intellij.ui.JBColor
import com.intellij.ui.components.JBLabel
import com.intellij.ui.content.ContentFactory
import com.intellij.util.ui.JBUI
import com.intellij.util.ui.UIUtil
import java.awt.BorderLayout
import javax.swing.JPanel
import javax.swing.SwingConstants

/** Hooks the preview service to the editor when a project opens. */
class KobraPreviewStartup : ProjectActivity {
    override suspend fun execute(project: Project) {
        ApplicationManager.getApplication().invokeLater({
            if (!project.isDisposed) KobraPreviewService.getInstance(project).install()
        }, project.disposed)
    }
}

/** The Kobra Preview tool window, on the right: the panel of `k preview` for the target file. */
class KobraPreviewToolWindowFactory : ToolWindowFactory, DumbAware {
    override fun createToolWindowContent(project: Project, toolWindow: ToolWindow) {
        val panel = KobraPreviewPanel(project)
        val content = ContentFactory.getInstance().createContent(panel, null, false)
        content.setDisposer(panel)
        toolWindow.contentManager.addContent(content)
    }
}

/** Whether the IDE's theme is dark, for the page's `theme=`. */
internal fun ideIsDark(): Boolean = !JBColor.isBright()

/**
 * The tool window's content: an error header (the current `error`, cleared by
 * `built`), the page in an embedded browser (or, without JCEF, a link to open
 * it in the browser), and a toolbar with Grid, Open in Browser and Restart.
 */
class KobraPreviewPanel(private val project: Project) : SimpleToolWindowPanel(true, true), Disposable {
    private val service = KobraPreviewService.getInstance(project)
    private val header = JBLabel().apply {
        foreground = JBColor.RED
        border = JBUI.Borders.empty(4, 8)
        isVisible = false
        setCopyable(true)
    }
    private val status = JBLabel("", SwingConstants.CENTER).apply {
        foreground = UIUtil.getContextHelpForeground()
    }
    private val browser: KobraPreviewBrowser? = KobraPreviewBrowserProvider.create()?.also { Disposer.register(this, it) }
    private val fallback = JPanel(BorderLayout()).apply {
        border = JBUI.Borders.empty(16)
        add(
            JBLabel(
                "<html>This IDE can't embed a browser (JCEF), so the preview opens in your browser.</html>",
            ),
            BorderLayout.NORTH,
        )
        add(
            HyperlinkLabel("Open in browser").apply {
                addHyperlinkListener { openInBrowser() }
            },
            BorderLayout.CENTER,
        )
    }
    private val body = JPanel(BorderLayout())

    /** Show every preview of the file at once (`mode=grid`). */
    var grid = false
        set(value) {
            field = value
            refresh()
        }

    private var loaded: PageKey? = null

    init {
        val actions = DefaultActionGroup(GridAction(), OpenInBrowserAction(), RestartAction())
        val toolbar = ActionManager.getInstance().createActionToolbar("KobraPreview", actions, true)
        toolbar.targetComponent = this
        setToolbar(toolbar.component)
        val main = JPanel(BorderLayout())
        main.add(header, BorderLayout.NORTH)
        main.add(body, BorderLayout.CENTER)
        setContent(main)
        service.addListener(this) { refresh() }
        ApplicationManager.getApplication().messageBus.connect(this)
            .subscribe(LafManagerListener.TOPIC, LafManagerListener { refresh() })
        refresh()
    }

    /** The page for the target, as it should be now. */
    private fun pageUrl(embed: Boolean = true): String? {
        val target = service.target ?: return null
        val session = service.targetSession() ?: return null
        return session.model.panelUrl(target.uri, target.index, if (embed) ideIsDark() else null, grid, embed)
    }

    private fun refresh() {
        val target = service.target
        val session = service.targetSession()
        val error = session?.model?.error
        val failure = session?.failure
        val problem = failure ?: error?.describe()
        header.text = problem?.let { "<html>${escape(it)}</html>" }.orEmpty()
        header.isVisible = problem != null

        val serverUrl = session?.model?.url
        val url = pageUrl()
        val shown = when {
            url == null || target == null || serverUrl == null -> {
                status.text = when {
                    target == null -> "Open a .k file with a preview, or click ▶ Preview above one."
                    else -> "Starting k preview…"
                }
                loaded = null
                status
            }
            browser == null -> fallback
            else -> {
                val key = PageKey(serverUrl, session.model.key(target.uri), grid, ideIsDark())
                if (key != loaded) {
                    loaded = key
                    browser.load(url)
                }
                browser.component
            }
        }
        if (body.componentCount != 1 || body.getComponent(0) !== shown) {
            body.removeAll()
            body.add(shown, BorderLayout.CENTER)
            body.revalidate()
            body.repaint()
        }
    }

    fun openInBrowser() {
        pageUrl(embed = false)?.let { BrowserUtil.browse(it) }
    }

    private fun escape(text: String) = text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")

    override fun dispose() {}

    private inner class GridAction : DumbAwareToggleAction("Grid", "Show every preview of the file at once", AllIcons.Actions.GroupByModule) {
        override fun isSelected(e: AnActionEvent) = grid

        override fun setSelected(e: AnActionEvent, state: Boolean) {
            grid = state
        }

        override fun getActionUpdateThread() = ActionUpdateThread.EDT
    }

    private inner class OpenInBrowserAction : DumbAwareAction("Open in Browser", "Open this preview in the default browser", AllIcons.General.Web) {
        override fun actionPerformed(e: AnActionEvent) = openInBrowser()

        override fun update(e: AnActionEvent) {
            e.presentation.isEnabled = pageUrl(embed = false) != null
        }

        override fun getActionUpdateThread() = ActionUpdateThread.EDT
    }

    private inner class RestartAction : DumbAwareAction("Restart k preview", "Restart the preview server, for example after installing a new k", AllIcons.Actions.Restart) {
        override fun actionPerformed(e: AnActionEvent) {
            loaded = null
            ApplicationManager.getApplication().executeOnPooledThread { service.restartAll() }
        }

        override fun getActionUpdateThread() = ActionUpdateThread.EDT
    }
}
