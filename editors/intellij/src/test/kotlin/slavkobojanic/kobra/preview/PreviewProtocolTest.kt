package slavkobojanic.kobra.preview

import com.google.gson.JsonParser
import org.eclipse.lsp4j.Command
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Path
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/** The editor protocol's messages, the model the panel reads, and the small pieces around them (no IDE needed). */
class PreviewProtocolTest {
    private val previewsLine =
        """{"type":"previews","uri":"file:///p/src/Counter.k","list":[""" +
            """{"index":0,"label":"Counter","line":7,"start":120,"end":127,"path":"/p/src/Counter.k/0"},""" +
            """{"index":1,"label":"at max","line":12,"start":200,"end":207,"path":"/p/src/Counter.k/1"}]}"""

    @Test
    fun parsesEveryMessageFromK() {
        assertEquals(PreviewEvent.Ready("http://127.0.0.1:4100/"), PreviewProtocol.parse("""{"type":"ready","url":"http://127.0.0.1:4100/"}"""))
        assertEquals(PreviewEvent.Built(3), PreviewProtocol.parse("""{"type":"built","build":3}"""))
        val previews = PreviewProtocol.parse(previewsLine) as PreviewEvent.Previews
        assertEquals("file:///p/src/Counter.k", previews.uri)
        assertEquals(PreviewEntry(1, "at max", 12, 200, 207, "/p/src/Counter.k/1"), previews.list[1])
        assertEquals(
            PreviewEvent.Error("file:///p/src/Counter.k", "expected `}`", 4, 9),
            PreviewProtocol.parse("""{"type":"error","uri":"file:///p/src/Counter.k","message":"expected `}`","line":4,"column":9}"""),
        )
        assertEquals(
            PreviewEvent.Error(null, "bad", null, null),
            PreviewProtocol.parse("""{"type":"error","uri":null,"message":"bad","line":null,"column":null}"""),
        )
    }

    @Test
    fun unknownAndBrokenLinesDontThrow() {
        assertNull(PreviewProtocol.parse("   "))
        assertTrue(PreviewProtocol.parse("not json") is PreviewEvent.Unknown)
        assertTrue(PreviewProtocol.parse("""{"type":"later"}""") is PreviewEvent.Unknown)
        assertTrue(PreviewProtocol.parse("""{"type":"ready"}""") is PreviewEvent.Unknown)
        assertTrue(PreviewProtocol.parse("[1]") is PreviewEvent.Unknown)
    }

    @Test
    fun writesTheEditorsMessages() {
        fun json(s: String) = JsonParser.parseString(s)
        assertEquals(json("""{"type":"didOpen","uri":"file:///a.k","text":"x\n\"y\""}"""), json(PreviewProtocol.didOpen("file:///a.k", "x\n\"y\"")))
        assertEquals(json("""{"type":"didChange","uri":"file:///a.k","text":""}"""), json(PreviewProtocol.didChange("file:///a.k", "")))
        assertEquals(json("""{"type":"didClose","uri":"file:///a.k"}"""), json(PreviewProtocol.didClose("file:///a.k")))
        assertEquals(json("""{"type":"select","uri":"file:///a.k","offset":12}"""), json(PreviewProtocol.select("file:///a.k", 12)))
        assertEquals(json("""{"type":"shutdown"}"""), json(PreviewProtocol.shutdown()))
        assertFalse("one line each", PreviewProtocol.didOpen("file:///a.k", "a\nb").contains('\n'))
    }

    @Test
    fun urisRoundTripAsKWritesThem() {
        assertEquals("file:///a/b%20c.k", PreviewProtocol.uriOf("/a/b c.k"))
        assertEquals("file:///C:/a/b.k", PreviewProtocol.uriOf("C:\\a\\b.k"))
        assertEquals("file:///a/50%25+x.k", PreviewProtocol.uriOf("/a/50%+x.k"))
        for (path in listOf("/a/b c.k", "/a/50%+x.k", "/a/ünï/Ço.k")) {
            assertEquals(path, PreviewProtocol.pathOf(PreviewProtocol.uriOf(path)))
        }
        assertNull(PreviewProtocol.pathOf("untitled:1"))
    }

    @Test
    fun theModelKeepsListsByNormalizedPath() {
        // `k` names the canonical root; the editor may name a symlink to it.
        val model = PreviewModel { it.replace(Regex("^/tmp/"), "/private/tmp/") }
        model.apply(PreviewEvent.Ready("http://127.0.0.1:4100/"))
        model.apply(PreviewProtocol.parse(previewsLine.replace("file:///p/", "file:///private/tmp/p/"))!!)
        assertTrue(model.hasPreviews("file:///tmp/p/src/Counter.k"))
        assertFalse(model.hasPreviews("file:///tmp/p/src/Other.k"))
        assertEquals(model.key("file:///tmp/p/src/Counter.k"), model.key("file:///private/tmp/p/src/Counter.k"))
    }

    @Test
    fun theCursorsPreviewIsTheLastKeywordBeforeIt() {
        val model = PreviewModel()
        model.apply(PreviewProtocol.parse(previewsLine)!!)
        val uri = "file:///p/src/Counter.k"
        assertNull(model.indexAt(uri, 0))
        assertNull(model.indexAt(uri, 119))
        assertEquals(0, model.indexAt(uri, 120))
        assertEquals(0, model.indexAt(uri, 199))
        assertEquals(1, model.indexAt(uri, 5000))
        assertNull(model.indexAt("file:///p/src/Other.k", 5000))
    }

    @Test
    fun thePanelUrlIsTheServersUrlAndThePath() {
        val model = PreviewModel()
        val uri = "file:///p/src/Counter.k"
        model.apply(PreviewProtocol.parse(previewsLine)!!)
        assertNull("not ready", model.panelUrl(uri, 0, dark = false, grid = false))
        model.apply(PreviewEvent.Ready("http://127.0.0.1:4100/"))
        assertEquals("http://127.0.0.1:4100/p/src/Counter.k/1?embed=1&theme=dark", model.panelUrl(uri, 1, dark = true, grid = false))
        assertEquals("http://127.0.0.1:4100/p/src/Counter.k/0?embed=1&theme=light&mode=grid", model.panelUrl(uri, 0, dark = false, grid = true))
        assertEquals("http://127.0.0.1:4100/p/src/Counter.k/0", model.panelUrl(uri, 0, dark = null, grid = false, embed = false))
        // A preview that is gone (renamed away) falls back to the file's first.
        assertEquals("http://127.0.0.1:4100/p/src/Counter.k/0?embed=1", model.panelUrl(uri, 9, dark = null, grid = false))
        assertNull(model.panelUrl("file:///p/src/Other.k", 0, dark = null, grid = false))
    }

    @Test
    fun builtClearsTheError() {
        val model = PreviewModel()
        val error = PreviewEvent.Error("file:///p/src/Counter.k", "expected `}`", 4, 9)
        assertTrue(model.apply(error))
        assertEquals("expected `}` (Counter.k:4:9)", model.error!!.describe())
        assertFalse("the same error again", model.apply(error))
        assertTrue(model.apply(PreviewEvent.Built(2)))
        assertNull(model.error)
        assertEquals(2L, model.build)
        assertEquals("bad", PreviewEvent.Error(null, "bad", null, null).describe())
    }

    @Test
    fun aPreviewKeywordStartsTheServer() {
        assertTrue(PreviewText.mayHavePreviews("component A {}\n\npreview {\n    A()\n}\n"))
        assertTrue(PreviewText.mayHavePreviews("  preview \"x\" (n: Int = 1) {}"))
        assertFalse(PreviewText.mayHavePreviews("let previews = 1\n// a preview of it\n"))
        assertFalse(PreviewText.mayHavePreviews("component A {}\n"))
        assertTrue(PreviewText.enablesPreviews("[package]\nname = \"p\"\npreview = [\"view-previews\"]\n"))
        assertFalse(PreviewText.enablesPreviews("[package]\nname = \"p\"\n"))
    }

    @Test
    fun theRootIsTheNearestKToml() {
        val files = setOf(Path.of("/w/k.toml"), Path.of("/w/apps/shop/k.toml"))
        val isFile = { p: Path -> p in files }
        assertEquals(Path.of("/w/apps/shop"), PreviewText.projectRoot(Path.of("/w/apps/shop/src/A.k"), isFile))
        assertEquals(Path.of("/w"), PreviewText.projectRoot(Path.of("/w/src/A.k"), isFile))
        assertNull(PreviewText.projectRoot(Path.of("/elsewhere/A.k"), isFile))
    }

    @Test
    fun backoffDoublesAndGivesUp() {
        val backoff = Backoff(baseMillis = 100, maxMillis = 1000, stableMillis = 10_000, maxQuickFailures = 5)
        assertEquals(listOf(100L, 200L, 400L, 800L, 1000L), (1..5).map { backoff.nextDelay(10) })
        assertNull(backoff.nextDelay(10))
        // A run that lasted starts over.
        assertEquals(100L, backoff.nextDelay(20_000))
    }

    @Test
    fun theThrottleRunsTheLatestOncePerDelay() {
        val scheduler = Executors.newSingleThreadScheduledExecutor()
        try {
            val ran = java.util.concurrent.CopyOnWriteArrayList<Int>()
            val throttle = Throttle(scheduler, 100)
            for (i in 1..5) throttle.submit { ran += i }
            scheduler.schedule({}, 300, TimeUnit.MILLISECONDS).get()
            assertEquals(listOf(5), ran.toList())
            throttle.submit { ran += 6 }
            throttle.cancel()
            scheduler.schedule({}, 200, TimeUnit.MILLISECONDS).get()
            assertEquals(listOf(5), ran.toList())
        } finally {
            scheduler.shutdownNow()
        }
    }

    @Test
    fun theCodeLensArgumentsAreReadFromGsonOrMaps() {
        val gson = Command("▶ Preview", KobraPreviewCommand.ID, listOf(JsonParser.parseString("""{"uri":"file:///p/A.k","index":2,"label":"at max"}""")))
        assertEquals(KobraPreviewCommand.Arguments("file:///p/A.k", 2, "at max"), KobraPreviewCommand.arguments(gson))
        val map = Command("▶ Preview", KobraPreviewCommand.ID, listOf(mapOf("uri" to "file:///p/A.k", "index" to 1.0)))
        assertEquals(KobraPreviewCommand.Arguments("file:///p/A.k", 1, null), KobraPreviewCommand.arguments(map))
        assertNull(KobraPreviewCommand.arguments(Command("x", "other.command", listOf())))
        assertNull(KobraPreviewCommand.arguments(Command("▶ Preview", KobraPreviewCommand.ID, listOf())))
    }
}
