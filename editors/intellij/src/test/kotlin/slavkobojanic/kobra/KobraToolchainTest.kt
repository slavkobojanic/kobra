package slavkobojanic.kobra

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path

/** Finding `k` (no IDE needed). */
class KobraToolchainTest {
    private fun env(path: String?, home: String? = "/home/u", kHome: String? = null) =
        KobraToolchain.Environment(path = path, home = home, kHome = kHome, windows = false)

    private fun exists(vararg paths: String): (Path) -> Boolean = { it.toString() in paths }

    @Test
    fun withNoSettingKOnPathComesFirst() {
        val found = KobraToolchain.find("", env("/a:/b"), exists("/b/k", "/home/u/.k/bin/k"))
        assertEquals(Path.of("/b/k"), found)
    }

    @Test
    fun thenTheInstallersBinDirectory() {
        assertEquals(Path.of("/home/u/.k/bin/k"), KobraToolchain.find("", env("/a"), exists("/home/u/.k/bin/k")))
        assertEquals(Path.of("/opt/k/bin/k"), KobraToolchain.find("", env("/a", kHome = "/opt/k"), exists("/opt/k/bin/k")))
        assertNull(KobraToolchain.find("", env(null), exists()))
    }

    @Test
    fun aConfiguredPathIsUsedAsItIs() {
        assertEquals(Path.of("/x/k"), KobraToolchain.find("/x/k", env("/b"), exists("/x/k", "/b/k")))
        assertEquals(Path.of("/home/u/bin/k"), KobraToolchain.find("~/bin/k", env("/b"), exists("/home/u/bin/k")))
        // Not found: no fallback to another `k`.
        assertNull(KobraToolchain.find("/x/k", env("/b"), exists("/b/k")))
    }

    @Test
    fun aBareNameIsLookedUpOnPath() {
        assertEquals(Path.of("/b/k-dev"), KobraToolchain.find("k-dev", env("/a:/b"), exists("/b/k-dev")))
    }

    @Test
    fun commentsComeFromTheVsCodeLanguageConfiguration() {
        val json = Files.readString(Path.of("../vscode/language-configuration.json"))
        assertEquals(CommentSyntax("//", "/*", "*/"), CommentSyntax.parse(json))
    }

    @Test
    fun theBracketsAreTheLanguageConfigurations() {
        val json = Files.readString(Path.of("../vscode/language-configuration.json"))
        val section = json.substringAfter("\"brackets\"").substringBefore("]\n  ]")
        val pairs = Regex(""""(.)"\s*,\s*"(.)"""").findAll(section).joinToString("") { it.groupValues[1] + it.groupValues[2] }
        assertEquals(BRACKETS.toSet(), pairs.toSet())
    }
}
