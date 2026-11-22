package slavkobojanic.kobra

import com.intellij.extapi.psi.ASTWrapperPsiElement
import com.intellij.extapi.psi.PsiFileBase
import com.intellij.lang.ASTNode
import com.intellij.lang.ParserDefinition
import com.intellij.lang.PsiParser
import com.intellij.lexer.Lexer
import com.intellij.lexer.LexerBase
import com.intellij.openapi.fileTypes.FileType
import com.intellij.openapi.project.Project
import com.intellij.psi.FileViewProvider
import com.intellij.psi.PsiElement
import com.intellij.psi.PsiFile
import com.intellij.psi.tree.IElementType
import com.intellij.psi.tree.IFileElementType
import com.intellij.psi.tree.TokenSet

/**
 * A flat PSI for Kobra files: the whole file is one token. The IDE needs a
 * PSI file to attach the editor features to; what the code means comes
 * from the language server, so there is no parser here (design 13, "JetBrains IDEs").
 */
class KobraParserDefinition : ParserDefinition {
    override fun createLexer(project: Project?): Lexer = WholeTextLexer()

    override fun createParser(project: Project?): PsiParser = PsiParser { root, builder ->
        val marker = builder.mark()
        while (!builder.eof()) {
            builder.advanceLexer()
        }
        marker.done(root)
        builder.treeBuilt
    }

    override fun getFileNodeType(): IFileElementType = FILE

    override fun getCommentTokens(): TokenSet = TokenSet.EMPTY

    override fun getStringLiteralElements(): TokenSet = TokenSet.EMPTY

    override fun createElement(node: ASTNode): PsiElement = ASTWrapperPsiElement(node)

    override fun createFile(viewProvider: FileViewProvider): PsiFile = KobraFile(viewProvider)

    companion object {
        val FILE = IFileElementType(KobraLanguage)
        val TEXT = IElementType("KOBRA_TEXT", KobraLanguage)
    }
}

class KobraFile(viewProvider: FileViewProvider) : PsiFileBase(viewProvider, KobraLanguage) {
    override fun getFileType(): FileType = KobraFileType.INSTANCE

    override fun toString(): String = "Kobra file"
}

/** One token, [KobraParserDefinition.TEXT], for the whole buffer (none for an empty one). */
private class WholeTextLexer : LexerBase() {
    private var buffer: CharSequence = ""
    private var start = 0
    private var end = 0
    private var done = false

    override fun start(buffer: CharSequence, startOffset: Int, endOffset: Int, initialState: Int) {
        this.buffer = buffer
        start = startOffset
        end = endOffset
        done = startOffset >= endOffset
    }

    override fun getState(): Int = 0

    override fun getTokenType(): IElementType? = if (done) null else KobraParserDefinition.TEXT

    override fun getTokenStart(): Int = start

    override fun getTokenEnd(): Int = end

    override fun advance() {
        done = true
        start = end
    }

    override fun getBufferSequence(): CharSequence = buffer

    override fun getBufferEnd(): Int = end
}
