package slavkobojanic.kobra

import com.intellij.lang.Language
import com.intellij.openapi.fileTypes.LanguageFileType
import com.intellij.openapi.util.IconLoader
import javax.swing.Icon

/** The Kobra language. Its analysis is `k lsp`'s; the IDE only lexes it, with the TextMate grammar. */
object KobraLanguage : Language("Kobra") {
    private fun readResolve(): Any = KobraLanguage

    override fun getDisplayName(): String = "Kobra"
}

/** Kobra source files, `*.k`. */
class KobraFileType private constructor() : LanguageFileType(KobraLanguage) {
    override fun getName(): String = "Kobra"

    override fun getDescription(): String = "Kobra source file"

    override fun getDefaultExtension(): String = EXTENSION

    override fun getIcon(): Icon = KobraIcons.FILE

    companion object {
        /** The extension of Kobra source files, without the dot. */
        const val EXTENSION = "k"

        @JvmField
        val INSTANCE = KobraFileType()
    }
}

object KobraIcons {
    @JvmField
    val FILE: Icon = IconLoader.getIcon("/icons/kobra.svg", KobraIcons::class.java)
}
