package slavkobojanic.kobra

import com.intellij.codeInsight.highlighting.BraceMatcher
import com.intellij.lexer.Lexer
import com.intellij.lexer.LexerBase
import com.intellij.openapi.editor.highlighter.HighlighterIterator
import com.intellij.openapi.fileTypes.FileType
import com.intellij.openapi.fileTypes.PlainSyntaxHighlighter
import com.intellij.openapi.fileTypes.SyntaxHighlighter
import com.intellij.openapi.fileTypes.SyntaxHighlighterFactory
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.psi.tree.IElementType
import org.jetbrains.plugins.textmate.editor.TextMateBraceMatcher
import org.jetbrains.plugins.textmate.language.syntax.highlighting.TextMateHighlighter
import org.jetbrains.plugins.textmate.language.syntax.highlighting.TextMateSyntaxHighlighterFactory

/**
 * TextMate's highlighter for the bundled grammar, with each bracket of
 * `language-configuration.json`'s `brackets` a token of its own: the
 * grammar leaves most braces inside a wider token, and the IDE matches
 * only a brace that is a whole token. The pieces keep their token's scope,
 * so the colours are the grammar's.
 */
class KobraSyntaxHighlighterFactory : SyntaxHighlighterFactory() {
    override fun getSyntaxHighlighter(project: Project?, virtualFile: VirtualFile?): SyntaxHighlighter {
        val inner = TextMateSyntaxHighlighterFactory().getSyntaxHighlighter(project, virtualFile)
        return if (inner is TextMateHighlighter) {
            KobraHighlighter(inner)
        } else {
            inner.takeUnless { it is PlainSyntaxHighlighter } ?: PlainSyntaxHighlighter()
        }
    }
}

class KobraHighlighter(private val inner: TextMateHighlighter) :
    TextMateHighlighter(BracketSplittingLexer(inner.highlightingLexer)) {
    override fun getTokenHighlights(tokenType: IElementType) = inner.getTokenHighlights(tokenType)
}

/** The brackets of `editors/vscode/language-configuration.json` (KobraToolchainTest checks they agree). */
const val BRACKETS = "{}[]()"

/**
 * Splits [inner]'s tokens so that each bracket outside strings and comments
 * is its own token of the same type. A piece after a token's first reports
 * the TextMate lexer's not-restartable state, so the editor restarts lexing
 * only where TextMate can.
 */
class BracketSplittingLexer(private val inner: Lexer) : LexerBase() {
    private var tokenStart = 0
    private var tokenEnd = 0
    private var firstPiece = true

    override fun start(buffer: CharSequence, startOffset: Int, endOffset: Int, initialState: Int) {
        inner.start(buffer, startOffset, endOffset, initialState)
        tokenStart = inner.tokenStart
        firstPiece = true
        tokenEnd = pieceEnd()
    }

    private fun splittable(): Boolean {
        val scope = inner.tokenType?.toString() ?: return false
        return !scope.contains("string") && !scope.contains("comment")
    }

    /** The end of the piece that starts at [tokenStart]. */
    private fun pieceEnd(): Int {
        val end = inner.tokenEnd
        if (inner.tokenType == null || !splittable()) return end
        val text = inner.bufferSequence
        if (BRACKETS.indexOf(text[tokenStart]) >= 0) return tokenStart + 1
        var at = tokenStart
        while (at < end && BRACKETS.indexOf(text[at]) < 0) at++
        return at
    }

    override fun getState(): Int = if (firstPiece) inner.state else NOT_RESTARTABLE

    override fun getTokenType(): IElementType? = inner.tokenType

    override fun getTokenStart(): Int = tokenStart

    override fun getTokenEnd(): Int = tokenEnd

    override fun advance() {
        if (tokenEnd >= inner.tokenEnd) {
            inner.advance()
            tokenStart = inner.tokenStart
            firstPiece = true
        } else {
            tokenStart = tokenEnd
            firstPiece = false
        }
        tokenEnd = pieceEnd()
    }

    override fun getBufferSequence(): CharSequence = inner.bufferSequence

    override fun getBufferEnd(): Int = inner.bufferEnd

    private companion object {
        /** TextMateHighlightingLexer's state where it can't restart. */
        const val NOT_RESTARTABLE = 1
    }
}

/**
 * TextMate's brace matcher, which pairs the brackets of the bundle's
 * language configuration, with two changes: its tokens are in one brace
 * group (TextMate's own puts them in none, -1, which the editor's matching
 * skips), and a bracket in a string or a comment isn't one, but for the
 * braces of a string interpolation.
 */
class KobraBraceMatcher : BraceMatcher by TextMateBraceMatcher() {
    private val inner = TextMateBraceMatcher()

    override fun getBraceTokenGroupId(tokenType: IElementType): Int = 0

    override fun isLBraceToken(iterator: HighlighterIterator, fileText: CharSequence, fileType: FileType): Boolean =
        code(iterator.tokenType) && inner.isLBraceToken(iterator, fileText, fileType)

    override fun isRBraceToken(iterator: HighlighterIterator, fileText: CharSequence, fileType: FileType): Boolean =
        code(iterator.tokenType) && inner.isRBraceToken(iterator, fileText, fileType)

    private fun code(type: IElementType?): Boolean {
        val scope = type?.toString() ?: return false
        return scope.contains("interpolation") || !(scope.contains("string") || scope.contains("comment"))
    }
}
