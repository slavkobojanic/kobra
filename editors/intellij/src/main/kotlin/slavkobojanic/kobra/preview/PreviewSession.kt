package slavkobojanic.kobra.preview

import java.io.InputStream
import java.io.OutputStream
import java.nio.file.Path
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit

/** A running `k preview --editor`, as [PreviewSession] needs it (a real process, or pipes in tests). */
interface PreviewProcess {
    /** `k`'s standard output: the protocol. */
    val output: InputStream

    /** `k`'s standard input. */
    val input: OutputStream

    fun destroy()

    /** `true` once the process has exited, waiting up to [millis]. */
    fun waitFor(millis: Long): Boolean
}

/**
 * One `k preview --editor` for one project root: starts it, sends it the open
 * buffers ([documents]) whenever it (re)starts, forwards edits and the
 * cursor, and restarts it with [Backoff] when it exits on its own. [stop]
 * sends `shutdown` and ends it.
 *
 * Events reach [onEvent] on the reading thread after the [model] has
 * recorded them; [onChanged] runs when the session starts, stops or gives up.
 */
class PreviewSession(
    val root: Path,
    private val launch: () -> PreviewProcess,
    private val documents: () -> List<Pair<String, String>>,
    private val scheduler: ScheduledExecutorService,
    private val onEvent: (PreviewSession, PreviewEvent) -> Unit = { _, _ -> },
    private val onChanged: (PreviewSession) -> Unit = {},
    normalize: (String) -> String = { it },
    private val backoff: Backoff = Backoff(),
    private val clock: () -> Long = System::currentTimeMillis,
) {
    val model = PreviewModel(normalize)

    /** Why there is no process (it couldn't start, or kept exiting); `null` while all is well. */
    @Volatile
    var failure: String? = null
        private set

    @Volatile
    private var client: PreviewClient? = null
    private var process: PreviewProcess? = null
    private var restart: ScheduledFuture<*>? = null
    private var stopped = false
    private var generation = 0

    val isRunning: Boolean get() = client?.isClosed == false

    @Synchronized
    fun start() {
        if (stopped || client?.isClosed == false) return
        restart?.cancel(false)
        restart = null
        val started = clock()
        val proc = try {
            launch()
        } catch (e: Exception) {
            failure = "Couldn't start k preview: ${e.message}"
            onChanged(this)
            return
        }
        failure = null
        model.reset()
        val mine = ++generation
        process = proc
        val c = PreviewClient(
            proc.output,
            proc.input,
            onEvent = { event ->
                model.apply(event)
                onEvent(this, event)
            },
            onClosed = { exited(mine, clock() - started) },
            name = "k preview ($root)",
        )
        client = c
        c.start()
        for ((uri, text) in documents()) c.didOpen(uri, text)
        onChanged(this)
    }

    private fun exited(which: Int, ranMillis: Long) {
        synchronized(this) {
            if (which != generation || stopped) return
            client?.close()
            client = null
            process?.destroy()
            process = null
            model.reset()
            val delay = backoff.nextDelay(ranMillis)
            if (delay == null) {
                failure = "k preview keeps exiting; see idea.log. Restart it from the Kobra Preview tool window."
            } else {
                failure = "k preview exited; restarting…"
                restart = scheduler.schedule({ start() }, delay, TimeUnit.MILLISECONDS)
            }
        }
        onChanged(this)
    }

    /** Starts again now, after giving up (the tool window's Restart). */
    @Synchronized
    fun restartNow() {
        backoff.reset()
        val old = client
        val oldProcess = process
        generation++
        client = null
        process = null
        old?.shutdown(200)
        oldProcess?.let { if (!it.waitFor(500)) it.destroy() }
        start()
    }

    fun didOpen(uri: String, text: String) {
        client?.didOpen(uri, text)
    }

    fun didChange(uri: String, text: String) {
        client?.didChange(uri, text)
    }

    fun didClose(uri: String) {
        client?.didClose(uri)
    }

    fun select(uri: String, offset: Int) {
        client?.select(uri, offset)
    }

    /** `shutdown`, then the process is ended if it hasn't stopped within [waitMillis]. */
    fun stop(waitMillis: Long = 2000) {
        val (c, p) = synchronized(this) {
            stopped = true
            restart?.cancel(false)
            restart = null
            generation++
            (client to process).also {
                client = null
                process = null
            }
        }
        c?.shutdown(waitMillis / 2)
        if (p != null && !p.waitFor(waitMillis / 2)) p.destroy()
    }
}
