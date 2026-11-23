package slavkobojanic.kobra.preview

import com.intellij.openapi.wm.ToolWindowAnchor
import com.intellij.openapi.wm.ToolWindowEP
import com.intellij.platform.lsp.api.LspClient
import com.intellij.testFramework.LightVirtualFile
import com.intellij.testFramework.fixtures.BasePlatformTestCase
import org.eclipse.lsp4j.Command
import slavkobojanic.kobra.KobraLspClientDescriptor
import slavkobojanic.kobra.KobraSettings
import java.lang.reflect.Proxy
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.attribute.PosixFilePermissions

/** The preview panel's registrations and the CodeLens command, in a headless IDE. */
class KobraPreviewPluginTest : BasePlatformTestCase() {
    private var dir: Path? = null

    override fun tearDown() {
        try {
            KobraPreviewService.getInstance(project).stopAll(1000)
            KobraSettings.getInstance().serverPath = ""
            KobraSettings.getInstance().autoOpenPreview = true
            dir?.toFile()?.deleteRecursively()
        } finally {
            super.tearDown()
        }
    }

    fun testTheToolWindowIsOnTheRight() {
        val ep = ToolWindowEP.EP_NAME.extensionList.single { it.id == KobraPreviewService.TOOL_WINDOW_ID }
        assertEquals(ToolWindowAnchor.RIGHT.toString(), ep.anchor)
        assertEquals(KobraPreviewToolWindowFactory::class.java.name, ep.factoryClass)
    }

    fun testAutoOpenIsOnByDefault() {
        assertTrue(KobraSettings.Options().autoOpenPreview)
    }

    fun testTheProcessIsKPreviewEditor() {
        val line = KobraPreviewService.commandLine(Path.of("/usr/bin/k"), Path.of("/w/shop"))
        assertEquals("/usr/bin/k", line.exePath)
        assertEquals(listOf("preview", "--editor", "--no-open", "/w/shop"), line.parametersList.list)
        assertEquals("/w/shop", line.workDirectory?.path)
    }

    fun testTheLanguageServerRunsThePreviewCommandInTheIde() {
        if (com.intellij.openapi.util.SystemInfo.isWindows) return
        // A project with a k.toml, and a fake `k` that answers `ready` and stops on `shutdown`.
        val root = Files.createTempDirectory("kobra-preview").toRealPath().also { dir = it }
        Files.writeString(root.resolve("k.toml"), "[package]\nname = \"p\"\npreview = [\"view-previews\"]\n")
        Files.createDirectories(root.resolve("src"))
        val file = root.resolve("src/A.k")
        Files.writeString(file, "preview { A() }\n")
        val k = root.resolve("k")
        Files.writeString(
            k,
            "#!/bin/sh\n" +
                "echo '{\"type\":\"ready\",\"url\":\"http://127.0.0.1:9/\"}'\n" +
                "while read line; do case \"\$line\" in *shutdown*) exit 0;; esac; done\n",
        )
        Files.setPosixFilePermissions(k, PosixFilePermissions.fromString("rwxr-xr-x"))
        KobraSettings.getInstance().serverPath = k.toString()

        val customization = KobraLspClientDescriptor(project).lspCustomization
        assertInstanceOf(customization, KobraLspCustomization::class.java)
        val client = Proxy.newProxyInstance(javaClass.classLoader, arrayOf(LspClient::class.java)) { _, method, _ ->
            if (method.name == "getProject") project else null
        } as LspClient
        val uri = PreviewProtocol.uriOf(file.toString())
        val command = Command("▶ Preview", KobraPreviewCommand.ID, listOf(com.google.gson.JsonParser.parseString("""{"uri":"$uri","index":1,"label":"x"}""")))
        val lens = customization.codeLensCustomizer as com.intellij.platform.lsp.api.customization.LspCodeLensSupport
        lens.codeLensClicked(client, LightVirtualFile("A.k", ""), command, null)

        val service = KobraPreviewService.getInstance(project)
        assertEquals(PreviewTarget(uri, 1), service.target)
        val session = service.session(root)
        assertNotNull("k preview started for the project root", session)
        val deadline = System.currentTimeMillis() + 5000
        while (session!!.model.url == null && System.currentTimeMillis() < deadline) Thread.sleep(20)
        assertEquals("http://127.0.0.1:9/", session.model.url)
    }
}
