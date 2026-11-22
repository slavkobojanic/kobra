package slavkobojanic.kobra

import com.intellij.lang.Commenter
import com.intellij.openapi.extensions.PluginAware
import com.intellij.openapi.extensions.PluginDescriptor
import org.jetbrains.plugins.textmate.api.TextMateBundleProvider
import java.nio.file.Files
import java.nio.file.Path

/**
 * The TextMate bundle the build copies from editors/vscode (the extension's
 * manifest, grammar and language configuration) into `<plugin>/textmate/kobra`.
 * Known once the IDE has created [KobraBundleProvider], which TextMate does
 * before it highlights anything.
 */
fun bundleDirectory(): Path? =
    KobraBundleProvider.plugin
        ?.pluginPath
        ?.resolve("textmate")
        ?.resolve("kobra")
        ?.takeIf { Files.isDirectory(it) }

/** Highlighting, brackets and auto-closing pairs: the VS Code extension's grammar, through TextMate. */
class KobraBundleProvider : TextMateBundleProvider, PluginAware {
    override fun setPluginDescriptor(pluginDescriptor: PluginDescriptor) {
        plugin = pluginDescriptor
    }

    override fun getBundles(): List<TextMateBundleProvider.PluginBundle> =
        listOfNotNull(bundleDirectory()?.let { TextMateBundleProvider.PluginBundle("Kobra", it) })

    companion object {
        /** This plugin, as the IDE tells the extension it creates. */
        @Volatile
        internal var plugin: PluginDescriptor? = null
    }
}

/** The comment syntax of `language-configuration.json`'s `comments`. */
data class CommentSyntax(val line: String?, val blockStart: String?, val blockEnd: String?) {
    companion object {
        /** Kobra's own, for a bundle that can't be read. */
        val DEFAULT = CommentSyntax("//", "/*", "*/")

        private val LINE = Regex(""""lineComment"\s*:\s*"((?:[^"\\]|\\.)*)"""")
        private val BLOCK = Regex(""""blockComment"\s*:\s*\[\s*"((?:[^"\\]|\\.)*)"\s*,\s*"((?:[^"\\]|\\.)*)"\s*]""")

        /** The `comments` of a VS Code language configuration. */
        fun parse(json: String): CommentSyntax {
            val block = BLOCK.find(json)
            return CommentSyntax(
                LINE.find(json)?.groupValues?.get(1),
                block?.groupValues?.get(1),
                block?.groupValues?.get(2),
            )
        }

        @Volatile
        private var bundled: CommentSyntax? = null

        /** The bundle's, read once; [DEFAULT] until the bundle is known. */
        val BUNDLED: CommentSyntax
            get() = bundled ?: (
                bundleDirectory()
                    ?.resolve("language-configuration.json")
                    ?.takeIf { Files.isRegularFile(it) }
                    ?.let { parse(Files.readString(it)) }
                    ?.also { bundled = it }
                    ?: DEFAULT
                )
    }
}

/** Comment with Line and Comment with Block, as the language configuration says. */
class KobraCommenter : Commenter {
    private val syntax get() = CommentSyntax.BUNDLED

    override fun getLineCommentPrefix(): String? = syntax.line

    override fun getBlockCommentPrefix(): String? = syntax.blockStart

    override fun getBlockCommentSuffix(): String? = syntax.blockEnd

    override fun getCommentedBlockCommentPrefix(): String? = null

    override fun getCommentedBlockCommentSuffix(): String? = null
}
