package slavkobojanic.kobra.preview

import java.nio.file.Files
import java.nio.file.Path
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit

/**
 * What one `k preview --editor` process has said: its URL, the last build,
 * the current error, and each file's previews. Files are keyed by
 * [normalize] of their path, so a URI `k` writes (from its canonical root)
 * finds the file the editor named through a symlink (`/tmp` on macOS).
 * Thread-safe: events arrive on the reading thread, questions come from the
 * UI thread.
 */
class PreviewModel(private val normalize: (String) -> String = { it }) {
    @get:Synchronized
    var url: String? = null
        private set

    @get:Synchronized
    var build: Long? = null
        private set

    @get:Synchronized
    var error: PreviewEvent.Error? = null
        private set

    private val lists = HashMap<String, List<PreviewEntry>>()

    fun key(uri: String): String = PreviewProtocol.pathOf(uri)?.let(normalize) ?: uri

    /** Records [event]; `true` when it changed what the panel shows. */
    @Synchronized
    fun apply(event: PreviewEvent): Boolean = when (event) {
        is PreviewEvent.Ready -> {
            url = event.url
            true
        }
        is PreviewEvent.Built -> {
            build = event.build
            val hadError = error != null
            error = null
            hadError
        }
        is PreviewEvent.Previews -> lists.put(key(event.uri), event.list) != event.list
        is PreviewEvent.Error -> {
            val changed = error != event
            error = event
            changed
        }
        is PreviewEvent.Unknown -> false
    }

    /** Forgets everything, when the process restarts. */
    @Synchronized
    fun reset() {
        url = null
        build = null
        error = null
        lists.clear()
    }

    /** The file's previews, in source order; empty until `k` has listed them. */
    @Synchronized
    fun previews(uri: String): List<PreviewEntry> = lists[key(uri)].orEmpty()

    fun hasPreviews(uri: String): Boolean = previews(uri).isNotEmpty()

    /**
     * The preview the cursor at [offset] is in, as far as the editor can tell:
     * the last whose keyword starts at or before it. `null` before the first
     * one. `k` decides exactly (it knows where each declaration ends); this is
     * for skipping `select`s that can't hit a preview and for remembering
     * which preview the panel shows.
     */
    @Synchronized
    fun indexAt(uri: String, offset: Int): Int? =
        previews(uri).lastOrNull { it.start <= offset }?.index

    /**
     * The page for the [index]th preview of [uri]: `url + path`, with
     * `embed=1` for the tool window, the IDE's `theme`, and `mode=grid`.
     * `null` until `k` is ready and has listed the file.
     */
    @Synchronized
    fun panelUrl(uri: String, index: Int, dark: Boolean?, grid: Boolean, embed: Boolean = true): String? {
        val base = url ?: return null
        val list = previews(uri)
        val entry = list.firstOrNull { it.index == index } ?: list.firstOrNull() ?: return null
        val query = buildList {
            if (embed) add("embed=1")
            if (dark != null) add("theme=" + if (dark) "dark" else "light")
            if (grid) add("mode=grid")
        }
        return base.trimEnd('/') + entry.path + if (query.isEmpty()) "" else "?" + query.joinToString("&")
    }
}

/**
 * What the panel shows: a file and one of its previews. The page is loaded
 * again only when [pageKey] changes (another file, another process, the grid
 * or the theme); moving between previews of the same file is a `select`, which
 * `k` relays to the open page.
 */
data class PreviewTarget(val uri: String, val index: Int)

data class PageKey(val serverUrl: String, val file: String, val grid: Boolean, val dark: Boolean)

/** Text that may hold a `preview` declaration: the cue to start `k preview` for its project. */
object PreviewText {
    private val keyword = Regex("""(?m)^[ \t]*preview\b""")

    fun mayHavePreviews(text: CharSequence): Boolean = keyword.containsMatchIn(text)

    /**
     * A project's `k.toml` that opts into previews (`preview = ["view-previews"]`):
     * elsewhere a `preview` is K8108, so no process is started for it.
     */
    fun enablesPreviews(kToml: String): Boolean = kToml.contains("\"view-previews\"")

    /** The directory with the `k.toml` nearest above [file], `null` outside a project. */
    fun projectRoot(file: Path, isFile: (Path) -> Boolean = Files::isRegularFile): Path? {
        var dir: Path? = file.parent
        while (dir != null) {
            if (isFile(dir.resolve("k.toml"))) return dir
            dir = dir.parent
        }
        return null
    }
}

/**
 * Restart delays after `k preview` exits on its own: 1 s, doubling to 30 s,
 * starting over once a run lasted [stableMillis]. After [maxQuickFailures]
 * quick exits in a row it gives up (`null`): something is wrong that a
 * restart won't fix, and the panel says so.
 */
class Backoff(
    private val baseMillis: Long = 1000,
    private val maxMillis: Long = 30_000,
    private val stableMillis: Long = 60_000,
    private val maxQuickFailures: Int = 6,
) {
    private var failures = 0

    fun nextDelay(ranMillis: Long): Long? {
        if (ranMillis >= stableMillis) failures = 0
        failures++
        if (failures > maxQuickFailures) return null
        return minOf(maxMillis, baseMillis shl (failures - 1).coerceAtMost(20))
    }

    fun reset() {
        failures = 0
    }
}

/**
 * At most one run of the latest action per [delayMillis] (trailing): the
 * cursor's `select`s while it moves.
 */
class Throttle(private val scheduler: ScheduledExecutorService, private val delayMillis: Long) {
    private var pending: (() -> Unit)? = null
    private var scheduled: ScheduledFuture<*>? = null

    @Synchronized
    fun submit(action: () -> Unit) {
        pending = action
        if (scheduled == null) {
            scheduled = scheduler.schedule({ fire() }, delayMillis, TimeUnit.MILLISECONDS)
        }
    }

    private fun fire() {
        val action = synchronized(this) {
            scheduled = null
            pending.also { pending = null }
        }
        action?.invoke()
    }

    @Synchronized
    fun cancel() {
        scheduled?.cancel(false)
        scheduled = null
        pending = null
    }
}
