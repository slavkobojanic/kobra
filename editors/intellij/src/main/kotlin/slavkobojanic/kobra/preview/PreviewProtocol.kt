package slavkobojanic.kobra.preview

import com.google.gson.JsonElement
import com.google.gson.JsonObject
import com.google.gson.JsonParser
import com.google.gson.JsonPrimitive

/**
 * The editor protocol of `k preview --editor` (docs/compiler/preview.md §5):
 * JSON objects, one per line. Offsets are UTF-16 code units, which is what a
 * Java string and an IntelliJ document count.
 */

/** One preview of a file, as a `previews` message lists it. */
data class PreviewEntry(
    val index: Int,
    val label: String,
    /** 1-based. */
    val line: Int,
    /** The `preview` keyword's UTF-16 offsets. */
    val start: Int,
    val end: Int,
    /** The page's path, `/p/src/shop/Counter.k/0`. */
    val path: String,
)

/** A message from `k`. */
sealed interface PreviewEvent {
    data class Ready(val url: String) : PreviewEvent

    data class Built(val build: Long) : PreviewEvent

    data class Previews(val uri: String, val list: List<PreviewEntry>) : PreviewEvent

    data class Error(val uri: String?, val message: String, val line: Int?, val column: Int?) : PreviewEvent {
        /** `message (Counter.k:3:5)`, for the panel's header. */
        fun describe(): String {
            val name = uri?.let { PreviewProtocol.pathOf(it) }?.substringAfterLast('/')
            val where = listOfNotNull(name, line?.toString(), column?.toString()).joinToString(":")
            return if (where.isEmpty()) message else "$message ($where)"
        }
    }

    /** A line that isn't a message this plugin knows (a newer `k`). */
    data class Unknown(val line: String) : PreviewEvent
}

object PreviewProtocol {
    /** A line from `k`'s standard output, or `null` for a blank one. */
    fun parse(line: String): PreviewEvent? {
        if (line.isBlank()) return null
        val json = try {
            JsonParser.parseString(line)
        } catch (_: Exception) {
            return PreviewEvent.Unknown(line)
        }
        if (json !is JsonObject) return PreviewEvent.Unknown(line)
        return try {
            when (json.string("type")) {
                "ready" -> PreviewEvent.Ready(json.string("url") ?: return PreviewEvent.Unknown(line))
                "built" -> PreviewEvent.Built(json.get("build")?.takeIf { it.isNumber() }?.asLong ?: 0)
                "previews" -> PreviewEvent.Previews(
                    json.string("uri") ?: return PreviewEvent.Unknown(line),
                    json.getAsJsonArray("list")?.mapNotNull { entry(it) }.orEmpty(),
                )
                "error" -> PreviewEvent.Error(
                    json.string("uri"),
                    json.string("message").orEmpty(),
                    json.int("line"),
                    json.int("column"),
                )
                else -> PreviewEvent.Unknown(line)
            }
        } catch (_: RuntimeException) {
            PreviewEvent.Unknown(line)
        }
    }

    private fun entry(element: JsonElement): PreviewEntry? {
        if (element !is JsonObject) return null
        return PreviewEntry(
            index = element.int("index") ?: return null,
            label = element.string("label").orEmpty(),
            line = element.int("line") ?: 0,
            start = element.int("start") ?: return null,
            end = element.int("end") ?: 0,
            path = element.string("path") ?: return null,
        )
    }

    private fun JsonElement.isNumber() = this is JsonPrimitive && this.isNumber

    private fun JsonObject.string(name: String): String? =
        get(name)?.takeIf { it is JsonPrimitive && it.isString }?.asString

    private fun JsonObject.int(name: String): Int? = get(name)?.takeIf { it.isNumber() }?.asInt

    private fun message(type: String, build: JsonObject.() -> Unit): String =
        JsonObject().apply {
            addProperty("type", type)
            build()
        }.toString()

    fun didOpen(uri: String, text: String) = message("didOpen") {
        addProperty("uri", uri)
        addProperty("text", text)
    }

    fun didChange(uri: String, text: String) = message("didChange") {
        addProperty("uri", uri)
        addProperty("text", text)
    }

    fun didClose(uri: String) = message("didClose") { addProperty("uri", uri) }

    fun select(uri: String, offset: Int) = message("select") {
        addProperty("uri", uri)
        addProperty("offset", offset)
    }

    fun shutdown() = message("shutdown") {}

    /**
     * The `file://` URI `k` expects for a local path (`/a/b c.k` is
     * `file:///a/b%20c.k`; `C:\a.k` is `file:///C:/a.k`). `k` percent-decodes
     * what follows `file://`.
     */
    fun uriOf(path: String): String {
        val slashed = path.replace('\\', '/')
        val rooted = if (slashed.startsWith("/")) slashed else "/$slashed"
        val encoded = buildString {
            for (c in rooted) {
                when (c) {
                    '%' -> append("%25")
                    ' ' -> append("%20")
                    '#' -> append("%23")
                    '?' -> append("%3F")
                    else -> append(c)
                }
            }
        }
        return "file://$encoded"
    }

    /** The local path of a `file://` URI (`/C:/a.k` stays as is; see [uriOf]). */
    fun pathOf(uri: String): String? {
        if (!uri.startsWith("file://")) return null
        return percentDecode(uri.removePrefix("file://"))
    }

    /** `%XX` escapes as UTF-8; a `+` stays a `+` (a path, not a form). */
    private fun percentDecode(text: String): String = try {
        java.net.URLDecoder.decode(text.replace("+", "%2B"), Charsets.UTF_8)
    } catch (_: IllegalArgumentException) {
        text
    }
}
