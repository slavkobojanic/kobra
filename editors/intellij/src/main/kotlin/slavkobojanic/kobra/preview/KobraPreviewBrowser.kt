package slavkobojanic.kobra.preview

import com.intellij.openapi.Disposable
import com.intellij.openapi.extensions.ExtensionPointName
import javax.swing.JComponent

/** The page in the Kobra Preview tool window: an embedded browser. */
interface KobraPreviewBrowser : Disposable {
    val component: JComponent

    fun load(url: String)
}

/**
 * Makes the embedded browser. The JCEF one is registered by
 * `kobra-jcef.xml`, loaded only when the IDE has the JCEF plugin
 * (`com.intellij.modules.jcef`), so the rest of the plugin works without it.
 */
interface KobraPreviewBrowserProvider {
    /** `null` when this IDE can't embed a browser (JCEF is off or unsupported). */
    fun create(): KobraPreviewBrowser?

    companion object {
        val EP: ExtensionPointName<KobraPreviewBrowserProvider> =
            ExtensionPointName.create("slavkobojanic.kobra.previewBrowser")

        fun create(): KobraPreviewBrowser? = EP.extensionList.firstNotNullOfOrNull {
            try {
                it.create()
            } catch (_: Exception) {
                null
            } catch (_: LinkageError) {
                null
            }
        }
    }
}
