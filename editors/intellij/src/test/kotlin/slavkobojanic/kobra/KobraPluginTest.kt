package slavkobojanic.kobra

import com.intellij.codeInsight.highlighting.BraceMatchingUtil
import com.intellij.execution.ExecutionException
import com.intellij.openapi.actionSystem.IdeActions
import com.intellij.openapi.fileTypes.FileTypeManager
import com.intellij.openapi.fileTypes.SyntaxHighlighterFactory
import com.intellij.testFramework.LightVirtualFile
import com.intellij.testFramework.fixtures.BasePlatformTestCase
import org.jetbrains.plugins.textmate.language.syntax.highlighting.TextMateHighlighter
import java.nio.file.Files
import java.nio.file.attribute.PosixFilePermissions

/** The plugin in a running (headless) IDE: the file type, the TextMate bundle, the editor features and the server's command line. */
class KobraPluginTest : BasePlatformTestCase() {
    override fun tearDown() {
        try {
            KobraSettings.getInstance().serverPath = ""
        } finally {
            super.tearDown()
        }
    }

    fun testKFilesAreKobra() {
        val type = FileTypeManager.getInstance().getFileTypeByFileName("App.k")
        assertSame(KobraFileType.INSTANCE, type)
        assertEquals("Kobra", type.name)
        assertEquals(KobraLanguage, KobraFileType.INSTANCE.language)
        val file = myFixture.configureByText("App.k", "component App {}\n")
        assertEquals(KobraLanguage, file.language)
    }

    fun testTheBundleIsTheVsCodeExtensions() {
        val dir = bundleDirectory()
        assertNotNull("the plugin's textmate/kobra directory", dir)
        for (name in listOf("package.json", "language-configuration.json", "syntaxes/k.tmLanguage.json")) {
            assertTrue(name, Files.isRegularFile(dir!!.resolve(name)))
        }
    }

    fun testTheTextMateLexerColoursASample() {
        val text = "// A counter.\ncomponent Counter {\n    @State var count: Int = 0\n    view { text { \"{count} clicks\" } }\n}\n"
        val file = LightVirtualFile("Counter.k", KobraFileType.INSTANCE, text)
        val highlighter = SyntaxHighlighterFactory.getSyntaxHighlighter(KobraFileType.INSTANCE, project, file)!!
        assertInstanceOf(highlighter, TextMateHighlighter::class.java)
        val lexer = highlighter.highlightingLexer
        lexer.start(text)
        val tokens = mutableListOf<Pair<String, String>>()
        var keyword: com.intellij.psi.tree.IElementType? = null
        while (lexer.tokenType != null) {
            val piece = text.substring(lexer.tokenStart, lexer.tokenEnd)
            if (piece == "component") keyword = lexer.tokenType
            tokens += piece to lexer.tokenType.toString()
            lexer.advance()
        }
        val shown = tokens.joinToString("\n") { (t, s) -> "${t.replace("\n", "\\n")} -> $s" }
        assertTrue(shown, tokens.size > 10)
        assertEquals("tokens cover the text", text, tokens.joinToString("") { it.first })
        fun scopeOf(piece: String) = tokens.firstOrNull { it.first.trim() == piece }?.second.orEmpty()
        assertTrue(shown, scopeOf("component").contains("storage.type.k"))
        assertTrue(shown, scopeOf("State").contains("annotation"))
        assertTrue(shown, scopeOf("view").contains("keyword"))
        assertTrue(shown, tokens.any { it.first.contains("A counter.") && it.second.contains("comment") })
        assertTrue(shown, tokens.any { it.first.contains("clicks") && it.second.contains("string") })
        // The keyword's scope maps to an editor colour.
        assertTrue(shown, highlighter.getTokenHighlights(keyword!!).isNotEmpty())
    }

    fun testCommentLineUsesTheLanguageConfiguration() {
        myFixture.configureByText("a.k", "let x = 1<caret>\n")
        myFixture.performEditorAction(IdeActions.ACTION_COMMENT_LINE)
        val line = myFixture.editor.document.text.lines().first()
        assertTrue(line, line.startsWith("//") && line.endsWith("let x = 1"))
        com.intellij.psi.PsiDocumentManager.getInstance(project).commitAllDocuments()
        myFixture.editor.caretModel.moveToOffset(0)
        myFixture.performEditorAction(IdeActions.ACTION_COMMENT_LINE)
        assertEquals("let x = 1", myFixture.editor.document.text.lines().first().trimStart())
    }

    fun testBracesMatch() {
        myFixture.configureByText("a.k", "func f() <caret>{\n    g(\"( {n}\", [1])\n}\n")
        val text = myFixture.editor.document.text
        fun matchAt(offset: Int): Int {
            myFixture.editor.caretModel.moveToOffset(offset)
            return BraceMatchingUtil.getMatchedBraceOffset(myFixture.editor, true, myFixture.file)
        }
        assertEquals(text.lastIndexOf('}'), matchAt(text.indexOf('{')))
        // The bracket inside the string isn't one; the interpolation's braces are.
        assertEquals(text.indexOf("])") + 1, matchAt(text.indexOf("g(") + 1))
        assertEquals(text.indexOf("n}") + 1, matchAt(text.indexOf("{n")))
        assertEquals(text.indexOf("])"), matchAt(text.indexOf("[1")))
    }

    fun testTheServerIsKLspFromTheSettings() {
        val k = Files.createTempFile("k", "")
        try {
            if (!com.intellij.openapi.util.SystemInfo.isWindows) {
                Files.setPosixFilePermissions(k, PosixFilePermissions.fromString("rwxr-xr-x"))
            }
            KobraSettings.getInstance().serverPath = k.toString()
            val line = KobraLspClientDescriptor(project).createCommandLine()
            assertEquals(k.toString(), line.exePath)
            assertEquals(listOf("lsp"), line.parametersList.list)
            assertEquals(project.basePath, line.workDirectory?.path)
            assertEquals("k", KobraLspClientDescriptor(project).getLanguageId(LightVirtualFile("App.k", "")))
            assertTrue(KobraLspClientDescriptor(project).isSupportedFile(LightVirtualFile("App.k", "")))
            assertFalse(KobraLspClientDescriptor(project).isSupportedFile(LightVirtualFile("App.kt", "")))
        } finally {
            Files.deleteIfExists(k)
        }
    }

    fun testAMissingKIsAnError() {
        KobraSettings.getInstance().serverPath = "/no/such/dir/k"
        try {
            KobraLspClientDescriptor(project).createCommandLine()
            fail("expected an ExecutionException")
        } catch (e: ExecutionException) {
            assertTrue(e.message, e.message!!.contains(KobraToolchain.INSTALL_SH))
        }
    }
}
