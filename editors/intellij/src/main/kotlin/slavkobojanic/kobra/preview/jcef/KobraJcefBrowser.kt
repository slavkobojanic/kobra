package slavkobojanic.kobra.preview.jcef

import com.intellij.ui.jcef.JBCefApp
import com.intellij.ui.jcef.JBCefBrowser
import slavkobojanic.kobra.preview.KobraPreviewBrowser
import slavkobojanic.kobra.preview.KobraPreviewBrowserProvider
import javax.swing.JComponent

/** The panel in a JCEF browser (registered by kobra-jcef.xml). */
class KobraJcefBrowserProvider : KobraPreviewBrowserProvider {
    override fun create(): KobraPreviewBrowser? =
        if (JBCefApp.isSupported()) KobraJcefBrowser() else null
}

private class KobraJcefBrowser : KobraPreviewBrowser {
    private val browser = JBCefBrowser()

    override val component: JComponent get() = browser.component

    override fun load(url: String) = browser.loadURL(url)

    override fun dispose() = browser.dispose()
}
