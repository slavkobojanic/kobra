package slavkobojanic.kobra.preview

import com.google.gson.JsonObject
import com.google.gson.JsonParser
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.PipedInputStream
import java.io.PipedOutputStream
import java.nio.file.Path
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

/** A fake `k preview --editor` over pipes: what the editor writes, and what "k" answers. */
private class FakeK : PreviewProcess {
    private val toEditor = PipedOutputStream()
    override val output = PipedInputStream(toEditor, 1 shl 16)
    private val fromEditor = PipedInputStream(1 shl 16)
    override val input = PipedOutputStream(fromEditor)
    val received = LinkedBlockingQueue<JsonObject>()
    private val exited = CountDownLatch(1)

    init {
        Thread({
            try {
                fromEditor.bufferedReader().forEachLine { received += JsonParser.parseString(it).asJsonObject }
            } catch (_: java.io.IOException) {
            }
        }, "fake k").apply { isDaemon = true }.start()
    }

    fun say(json: String) {
        toEditor.write((json + "\n").toByteArray())
        toEditor.flush()
    }

    /** The next message from the editor, within a second. */
    fun next(): JsonObject = received.poll(2, TimeUnit.SECONDS) ?: error("no message from the editor")

    fun exit() {
        try {
            toEditor.close()
        } catch (_: java.io.IOException) {
        }
        exited.countDown()
    }

    override fun destroy() = exit()

    override fun waitFor(millis: Long) = exited.await(millis, TimeUnit.MILLISECONDS)
}

class PreviewSessionTest {
    private val scheduler = Executors.newSingleThreadScheduledExecutor()

    @After
    fun stopScheduler() {
        scheduler.shutdownNow()
    }

    private fun JsonObject.type() = get("type").asString

    @Test
    fun theClientWritesInOrderAndReadsEveryEvent() {
        val k = FakeK()
        val events = LinkedBlockingQueue<PreviewEvent>()
        val closed = CountDownLatch(1)
        val client = PreviewClient(k.output, k.input, { events += it }, { closed.countDown() }).start()
        client.didOpen("file:///p/A.k", "preview {\n    A()\n}\n")
        client.didChange("file:///p/A.k", "preview { B() }")
        client.select("file:///p/A.k", 3)
        client.didClose("file:///p/A.k")
        assertEquals(listOf("didOpen", "didChange", "select", "didClose"), (1..4).map { k.next().type() })

        k.say("""{"type":"ready","url":"http://127.0.0.1:1/"}""")
        k.say("not json")
        k.say("""{"type":"built","build":1}""")
        assertEquals(PreviewEvent.Ready("http://127.0.0.1:1/"), events.poll(2, TimeUnit.SECONDS))
        assertTrue(events.poll(2, TimeUnit.SECONDS) is PreviewEvent.Unknown)
        assertEquals(PreviewEvent.Built(1), events.poll(2, TimeUnit.SECONDS))

        client.shutdown(1000)
        assertEquals("shutdown", k.next().type())
        k.exit()
        assertTrue("onClosed once the output ends", closed.await(2, TimeUnit.SECONDS))
        assertTrue(client.isClosed)
        client.didOpen("file:///p/A.k", "dropped")
        assertNull(k.received.poll(100, TimeUnit.MILLISECONDS))
    }

    @Test
    fun aSessionOpensTheBuffersRecordsEventsAndRestartsAfterACrash() {
        val launched = LinkedBlockingQueue<FakeK>()
        val events = CopyOnWriteArrayList<PreviewEvent>()
        val docs = listOf("file:///p/src/A.k" to "preview { A() }", "file:///p/src/B.k" to "component B {}")
        val session = PreviewSession(
            root = Path.of("/p"),
            launch = { FakeK().also { launched += it } },
            documents = { docs },
            scheduler = scheduler,
            onEvent = { _, e -> events += e },
            backoff = Backoff(baseMillis = 50, stableMillis = 60_000),
        )
        session.start()
        val first = launched.poll(2, TimeUnit.SECONDS)!!
        assertEquals(listOf("file:///p/src/A.k", "file:///p/src/B.k"), (1..2).map { first.next().also { m -> assertEquals("didOpen", m.type()) }.get("uri").asString })
        assertTrue(session.isRunning)

        first.say("""{"type":"ready","url":"http://127.0.0.1:7/"}""")
        first.say("""{"type":"previews","uri":"file:///p/src/A.k","list":[{"index":0,"label":"A","line":1,"start":0,"end":7,"path":"/p/src/A.k/0"}]}""")
        first.say("""{"type":"error","uri":"file:///p/src/A.k","message":"oops","line":1,"column":2}""")
        waitUntil { events.size == 3 }
        assertEquals("http://127.0.0.1:7/p/src/A.k/0?embed=1", session.model.panelUrl("file:///p/src/A.k", 0, null, false))
        assertEquals("oops", session.model.error?.message)

        session.didChange("file:///p/src/A.k", "preview { A(n: 1) }")
        session.select("file:///p/src/A.k", 2)
        assertEquals("didChange", first.next().type())
        assertEquals(2, first.next().get("offset").asInt)

        // k crashes: the session forgets its state, restarts after the backoff and sends the buffers again.
        first.exit()
        val second = launched.poll(2, TimeUnit.SECONDS)
        assertNotNull("restarted", second)
        assertEquals("didOpen", second!!.next().type())
        assertEquals("didOpen", second.next().type())
        assertNull("the old URL is gone", session.model.url)
        waitUntil { session.isRunning }

        // Stopping sends shutdown and doesn't restart.
        val stopper = Thread { session.stop(400) }
        stopper.start()
        assertEquals("shutdown", second.next().type())
        second.exit()
        stopper.join(2000)
        assertFalse(session.isRunning)
        assertNull("no restart after stop", launched.poll(300, TimeUnit.MILLISECONDS))
    }

    @Test
    fun aSessionGivesUpAfterQuickExits() {
        var launches = 0
        val changed = CopyOnWriteArrayList<String?>()
        val session = PreviewSession(
            root = Path.of("/p"),
            launch = { launches++; FakeK().also { it.exit() } },
            documents = { emptyList() },
            scheduler = scheduler,
            onChanged = { changed += it.failure },
            backoff = Backoff(baseMillis = 10, maxQuickFailures = 2),
        )
        session.start()
        waitUntil { session.failure?.contains("keeps exiting") == true }
        assertEquals(3, launches)
        session.stop(100)
    }

    @Test
    fun aLaunchThatFailsIsReported() {
        val session = PreviewSession(
            root = Path.of("/p"),
            launch = { throw java.io.IOException("no such file") },
            documents = { emptyList() },
            scheduler = scheduler,
        )
        session.start()
        assertEquals("Couldn't start k preview: no such file", session.failure)
        assertFalse(session.isRunning)
        session.didOpen("file:///p/A.k", "ignored")
    }

    private fun waitUntil(condition: () -> Boolean) {
        val deadline = System.currentTimeMillis() + 3000
        while (!condition()) {
            if (System.currentTimeMillis() > deadline) error("timed out")
            Thread.sleep(10)
        }
    }
}
