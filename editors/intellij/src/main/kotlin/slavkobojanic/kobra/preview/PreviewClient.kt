package slavkobojanic.kobra.preview

import java.io.BufferedReader
import java.io.IOException
import java.io.InputStream
import java.io.InputStreamReader
import java.io.OutputStream
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/**
 * The editor's end of `k preview --editor`, over its standard output
 * ([fromK]) and standard input ([toK]): messages are written in order on a
 * thread of their own, so a listener on the UI thread never blocks on the
 * pipe, and each line `k` writes is parsed and handed to [onEvent] on the
 * reading thread. [onClosed] runs once, when `k`'s output ends (it exited or
 * crashed) or after [close].
 *
 * Nothing here knows about the IDE, so it is tested over pipes.
 */
class PreviewClient(
    private val fromK: InputStream,
    private val toK: OutputStream,
    private val onEvent: (PreviewEvent) -> Unit,
    private val onClosed: () -> Unit = {},
    name: String = "k preview",
) : AutoCloseable {
    private val writer: ExecutorService = Executors.newSingleThreadExecutor { r ->
        Thread(r, "$name writer").apply { isDaemon = true }
    }
    private val reader = Thread({ read() }, "$name reader").apply { isDaemon = true }
    private val closed = AtomicBoolean(false)
    private val ended = AtomicBoolean(false)

    fun start(): PreviewClient {
        reader.start()
        return this
    }

    val isClosed: Boolean get() = closed.get() || ended.get()

    private fun read() {
        try {
            BufferedReader(InputStreamReader(fromK, Charsets.UTF_8)).use { lines ->
                while (true) {
                    val line = lines.readLine() ?: break
                    val event = PreviewProtocol.parse(line) ?: continue
                    try {
                        onEvent(event)
                    } catch (_: RuntimeException) {
                        // A listener's failure doesn't stop the stream.
                    }
                }
            }
        } catch (_: IOException) {
            // The process went away.
        } finally {
            end()
        }
    }

    private fun end() {
        if (ended.compareAndSet(false, true)) {
            onClosed()
        }
    }

    /** Queues one message (a line of JSON); dropped once the client is closed. */
    fun send(message: String) {
        if (isClosed) return
        try {
            writer.execute {
                try {
                    toK.write((message + "\n").toByteArray(Charsets.UTF_8))
                    toK.flush()
                } catch (_: IOException) {
                    // `k` exited: the reader sees the end and reports it.
                }
            }
        } catch (_: java.util.concurrent.RejectedExecutionException) {
            // Closed meanwhile.
        }
    }

    fun didOpen(uri: String, text: String) = send(PreviewProtocol.didOpen(uri, text))

    fun didChange(uri: String, text: String) = send(PreviewProtocol.didChange(uri, text))

    fun didClose(uri: String) = send(PreviewProtocol.didClose(uri))

    fun select(uri: String, offset: Int) = send(PreviewProtocol.select(uri, offset))

    /**
     * Sends `shutdown`, waits up to [waitMillis] for the queue to drain, and
     * closes `k`'s standard input (which stops it too).
     */
    fun shutdown(waitMillis: Long = 1000) {
        send(PreviewProtocol.shutdown())
        close(waitMillis)
    }

    /** Stops writing and closes `k`'s standard input; [onClosed] follows when its output ends. */
    fun close(waitMillis: Long) {
        if (!closed.compareAndSet(false, true)) return
        writer.shutdown()
        try {
            writer.awaitTermination(waitMillis, TimeUnit.MILLISECONDS)
        } catch (_: InterruptedException) {
            Thread.currentThread().interrupt()
        }
        try {
            toK.close()
        } catch (_: IOException) {
        }
    }

    override fun close() = close(0)
}
