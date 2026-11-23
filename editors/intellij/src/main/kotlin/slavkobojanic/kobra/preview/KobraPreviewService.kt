package slavkobojanic.kobra.preview

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.openapi.Disposable
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.application.ModalityState
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.service
import com.intellij.openapi.diagnostic.logger
import com.intellij.openapi.editor.Document
import com.intellij.openapi.editor.Editor
import com.intellij.openapi.editor.EditorFactory
import com.intellij.openapi.editor.event.CaretEvent
import com.intellij.openapi.editor.event.CaretListener
import com.intellij.openapi.editor.event.DocumentEvent
import com.intellij.openapi.editor.event.DocumentListener
import com.intellij.openapi.fileEditor.FileDocumentManager
import com.intellij.openapi.fileEditor.FileEditorManager
import com.intellij.openapi.fileEditor.FileEditorManagerEvent
import com.intellij.openapi.fileEditor.FileEditorManagerListener
import com.intellij.openapi.fileEditor.TextEditor
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.Computable
import com.intellij.openapi.util.Disposer
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.openapi.wm.ToolWindowManager
import com.intellij.util.concurrency.AppExecutorUtil
import slavkobojanic.kobra.KobraSettings
import slavkobojanic.kobra.KobraToolchain
import slavkobojanic.kobra.isKobra
import java.io.InputStream
import java.io.OutputStream
import java.nio.charset.StandardCharsets
import java.nio.file.Path
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.TimeUnit

/**
 * The project's `k preview --editor` processes, one per project root (the
 * directory of the nearest `k.toml`), started when a `.k` file that may hold
 * a `preview` is opened (docs/design/57-previews.md, "The panel"). It sends
 * each process the open buffers, every edit and, throttled, the cursor in a
 * file with previews; it keeps what the panel shows ([target]) and shows the
 * Kobra Preview tool window for a selected file with previews.
 */
@Service(Service.Level.PROJECT)
class KobraPreviewService(private val project: Project) : Disposable {
    private val sessions = ConcurrentHashMap<Path, PreviewSession>()
    private val listeners = CopyOnWriteArrayList<() -> Unit>()
    private val scheduler = AppExecutorUtil.getAppScheduledExecutorService()
    private val selects = Throttle(scheduler, SELECT_DELAY_MILLIS)

    /** The file and preview the panel shows; `null` before any. */
    @Volatile
    var target: PreviewTarget? = null
        private set

    /** The file the editor's selection last auto-showed the tool window for. */
    private var autoShown: String? = null

    @Volatile
    private var disposed = false

    fun session(root: Path): PreviewSession? = sessions[root]

    /** The session of the panel's file. */
    fun targetSession(): PreviewSession? = target?.let { sessionOf(it.uri) }

    private fun sessionOf(uri: String): PreviewSession? {
        val path = PreviewProtocol.pathOf(uri)?.let { runCatching { Path.of(it) }.getOrNull() } ?: return null
        return PreviewText.projectRoot(path)?.let { sessions[it] }
    }

    /** Calls [listener] on the UI thread whenever what the panel shows may have changed. */
    fun addListener(parent: Disposable, listener: () -> Unit) {
        listeners += listener
        Disposer.register(parent) { listeners -= listener }
    }

    private fun changed() {
        ApplicationManager.getApplication().invokeLater({
            if (!project.isDisposed) listeners.forEach { it() }
        }, ModalityState.any())
    }

    /** Hooks the editor's events (once, from [KobraPreviewStartup]) and opens the files already open. */
    fun install() {
        val multicaster = EditorFactory.getInstance().eventMulticaster
        multicaster.addDocumentListener(object : DocumentListener {
            override fun documentChanged(event: DocumentEvent) = documentChanged(event.document)
        }, this)
        multicaster.addCaretListener(object : CaretListener {
            override fun caretPositionChanged(event: CaretEvent) = caretMoved(event.editor)
        }, this)
        project.messageBus.connect(this).subscribe(
            FileEditorManagerListener.FILE_EDITOR_MANAGER,
            object : FileEditorManagerListener {
                override fun fileOpened(source: FileEditorManager, file: VirtualFile) = opened(file)

                override fun fileClosed(source: FileEditorManager, file: VirtualFile) = closed(file)

                override fun selectionChanged(event: FileEditorManagerEvent) = selected(event.newFile)
            },
        )
        val editors = FileEditorManager.getInstance(project)
        for (file in editors.openFiles) opened(file)
        selected(editors.selectedFiles.firstOrNull())
    }

    // The editor's events.

    private fun localPath(file: VirtualFile): Path? =
        if (isKobra(file) && file.isInLocalFileSystem) file.toNioPathOrNull() else null

    private fun VirtualFile.toNioPathOrNull(): Path? = try {
        toNioPath()
    } catch (_: UnsupportedOperationException) {
        null
    }

    private fun uriOf(path: Path) = PreviewProtocol.uriOf(path.toString())

    private fun textOf(file: VirtualFile): String? = ApplicationManager.getApplication().runReadAction(
        Computable { FileDocumentManager.getInstance().getDocument(file)?.immutableCharSequence?.toString() },
    )

    /** The session for [path]'s project, started if [text] may hold a preview. */
    private fun sessionFor(path: Path, text: CharSequence?): PreviewSession? {
        val root = PreviewText.projectRoot(path) ?: return null
        sessions[root]?.let { return it }
        if (text == null || !PreviewText.mayHavePreviews(text) || disposed) return null
        val toml = try {
            java.nio.file.Files.readString(root.resolve("k.toml"))
        } catch (_: java.io.IOException) {
            return null
        }
        if (!PreviewText.enablesPreviews(toml)) return null
        if (KobraToolchain.find(KobraSettings.getInstance().serverPath) == null) return null
        return sessions.computeIfAbsent(root) { newSession(it) }.also { it.start() }
    }

    private fun opened(file: VirtualFile) {
        val path = localPath(file) ?: return
        val text = textOf(file) ?: return
        val existing = PreviewText.projectRoot(path)?.let { sessions[it] }
        val session = sessionFor(path, text) ?: return
        // A new session sent every open buffer when it started.
        if (existing != null) session.didOpen(uriOf(path), text)
    }

    private fun closed(file: VirtualFile) {
        val path = localPath(file) ?: return
        val root = PreviewText.projectRoot(path) ?: return
        sessions[root]?.didClose(uriOf(path))
    }

    private fun documentChanged(document: Document) {
        val file = FileDocumentManager.getInstance().getFile(document) ?: return
        val path = localPath(file) ?: return
        if (!FileEditorManager.getInstance(project).isFileOpen(file)) return
        val text = document.immutableCharSequence
        val existing = PreviewText.projectRoot(path)?.let { sessions[it] }
        val session = sessionFor(path, text) ?: return
        if (existing != null) session.didChange(uriOf(path), text.toString())
    }

    private fun caretMoved(editor: Editor) {
        if (editor.project != project) return
        val file = FileDocumentManager.getInstance().getFile(editor.document) ?: return
        val path = localPath(file) ?: return
        val root = PreviewText.projectRoot(path) ?: return
        val session = sessions[root] ?: return
        val uri = uriOf(path)
        val offset = editor.caretModel.offset
        val index = session.model.indexAt(uri, offset) ?: return
        selects.submit {
            session.select(uri, offset)
            val t = target
            if (t != null && session.model.key(t.uri) == session.model.key(uri) && t.index != index) {
                target = PreviewTarget(t.uri, index)
                changed()
            }
        }
    }

    private fun selected(file: VirtualFile?) {
        autoShown = null
        val path = file?.let { localPath(it) } ?: return
        val root = PreviewText.projectRoot(path) ?: return
        val session = sessions[root] ?: return
        follow(session, uriOf(path))
    }

    /**
     * The panel follows the selected editor: a file with previews becomes the
     * target (at the preview the cursor is in), and the tool window shows
     * once per selection, as the setting allows.
     */
    private fun follow(session: PreviewSession, uri: String) {
        if (!session.model.hasPreviews(uri)) return
        val key = session.model.key(uri)
        if (target?.let { session.model.key(it.uri) } != key) {
            target = PreviewTarget(uri, caretIndex(session, uri) ?: 0)
            changed()
        }
        if (autoShown != key && KobraSettings.getInstance().autoOpenPreview) {
            autoShown = key
            ApplicationManager.getApplication().invokeLater({
                if (!project.isDisposed) ToolWindowManager.getInstance(project).getToolWindow(TOOL_WINDOW_ID)?.show()
            }, ModalityState.nonModal())
        }
    }

    private fun caretIndex(session: PreviewSession, uri: String): Int? {
        val editor = FileEditorManager.getInstance(project).selectedEditor as? TextEditor ?: return null
        val file = FileDocumentManager.getInstance().getFile(editor.editor.document) ?: return null
        val path = localPath(file) ?: return null
        if (session.model.key(uriOf(path)) != session.model.key(uri)) return null
        return session.model.indexAt(uri, editor.editor.caretModel.offset)
    }

    private fun selectedUri(): String? {
        val file = FileEditorManager.getInstance(project).selectedFiles.firstOrNull() ?: return null
        return localPath(file)?.let { uriOf(it) }
    }

    // Opening a preview (the CodeLens).

    /** Shows the [index]th preview of [uri] in the tool window (the **▶ Preview** CodeLens). */
    fun open(uri: String, index: Int) {
        val path = PreviewProtocol.pathOf(uri)?.let { runCatching { Path.of(it) }.getOrNull() } ?: return
        val session = sessionFor(path, "preview") ?: return
        val previous = target
        target = PreviewTarget(uri, index)
        if (previous != null && session.model.key(previous.uri) == session.model.key(uri)) {
            // Same page: `k` moves it, without a reload.
            session.model.previews(uri).firstOrNull { it.index == index }?.let { session.select(uri, it.start) }
        }
        changed()
        ApplicationManager.getApplication().invokeLater({
            if (!project.isDisposed) ToolWindowManager.getInstance(project).getToolWindow(TOOL_WINDOW_ID)?.activate(null, false)
        }, ModalityState.nonModal())
    }

    /** Restarts every process (after the settings change, or the tool window's Restart). */
    fun restartAll() {
        for (session in sessions.values) session.restartNow()
    }

    // The sessions.

    private fun newSession(root: Path) = PreviewSession(
        root = root,
        launch = { launch(root) },
        documents = { openDocuments(root) },
        scheduler = scheduler,
        onEvent = { session, event -> event(session, event) },
        onChanged = { changed() },
        normalize = ::normalize,
    )

    private fun event(session: PreviewSession, event: PreviewEvent) {
        when (event) {
            is PreviewEvent.Previews -> {
                val selected = selectedUri()
                if (selected != null && session.model.key(selected) == session.model.key(event.uri)) {
                    ApplicationManager.getApplication().invokeLater({
                        if (!project.isDisposed) follow(session, selected)
                    }, ModalityState.any())
                }
                changed()
            }
            is PreviewEvent.Ready, is PreviewEvent.Built, is PreviewEvent.Error -> changed()
            is PreviewEvent.Unknown -> LOG.debug("k preview: ${event.line}")
        }
    }

    private fun openDocuments(root: Path): List<Pair<String, String>> =
        FileEditorManager.getInstance(project).openFiles.mapNotNull { file ->
            val path = localPath(file) ?: return@mapNotNull null
            if (PreviewText.projectRoot(path) != root) return@mapNotNull null
            textOf(file)?.let { uriOf(path) to it }
        }

    private fun launch(root: Path): PreviewProcess {
        val k = KobraToolchain.find(KobraSettings.getInstance().serverPath)
            ?: throw IllegalStateException("`k` wasn't found")
        val process = commandLine(k, root).createProcess()
        Thread({ drain(process.errorStream, root) }, "k preview stderr ($root)").apply { isDaemon = true }.start()
        return object : PreviewProcess {
            override val output: InputStream get() = process.inputStream
            override val input: OutputStream get() = process.outputStream

            override fun destroy() {
                process.destroy()
            }

            override fun waitFor(millis: Long) = process.waitFor(millis, TimeUnit.MILLISECONDS)
        }
    }

    private fun drain(stderr: InputStream, root: Path) {
        try {
            stderr.bufferedReader(StandardCharsets.UTF_8).useLines { lines ->
                lines.forEach { LOG.info("k preview ($root): $it") }
            }
        } catch (_: java.io.IOException) {
        }
    }

    override fun dispose() {
        disposed = true
        selects.cancel()
        // Stopping waits briefly for each process; don't hold the UI thread for it.
        try {
            ApplicationManager.getApplication().executeOnPooledThread { stopAll() }
        } catch (_: java.util.concurrent.RejectedExecutionException) {
            stopAll(200)
        }
    }

    /** Sends every process `shutdown` and forgets them (also when the project closes). */
    fun stopAll(waitMillis: Long = 2000) {
        val all = sessions.values.toList()
        sessions.clear()
        all.forEach { it.stop(waitMillis) }
        target = null
    }

    companion object {
        const val TOOL_WINDOW_ID = "Kobra Preview"
        const val SELECT_DELAY_MILLIS = 150L
        private val LOG = logger<KobraPreviewService>()
        private val realPaths = ConcurrentHashMap<String, String>()

        fun getInstance(project: Project): KobraPreviewService = project.service()

        /** `k preview --editor --no-open <root>`, run in the root. */
        fun commandLine(k: Path, root: Path): GeneralCommandLine =
            GeneralCommandLine(k.toString(), "preview", "--editor", "--no-open", root.toString())
                .withCharset(StandardCharsets.UTF_8)
                .withParentEnvironmentType(GeneralCommandLine.ParentEnvironmentType.CONSOLE)
                .withWorkDirectory(root.toFile())

        /** A path as `k` names it: its real path where it exists (`k` canonicalizes the root). */
        fun normalize(path: String): String = realPaths[path] ?: try {
            Path.of(path).toRealPath().toString().also { realPaths[path] = it }
        } catch (_: Exception) {
            try {
                Path.of(path).normalize().toString()
            } catch (_: Exception) {
                path
            }
        }
    }
}
