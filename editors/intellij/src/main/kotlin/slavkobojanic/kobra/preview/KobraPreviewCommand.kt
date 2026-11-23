package slavkobojanic.kobra.preview

import com.google.gson.JsonElement
import com.google.gson.JsonObject
import com.google.gson.JsonParser
import com.google.gson.JsonPrimitive
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.platform.lsp.api.LspClient
import com.intellij.platform.lsp.api.customization.LspCodeLensCustomizer
import com.intellij.platform.lsp.api.customization.LspCodeLensSupport
import com.intellij.platform.lsp.api.customization.LspCommandsCustomizer
import com.intellij.platform.lsp.api.customization.LspCommandsSupport
import com.intellij.platform.lsp.api.customization.LspCustomization
import org.eclipse.lsp4j.Command
import java.awt.event.MouseEvent

/**
 * `kobra.preview.open`, the command of `k lsp`'s **▶ Preview** CodeLens
 * (docs/compiler/preview.md): a client-side command, so the plugin runs it
 * instead of sending it back to the server.
 */
object KobraPreviewCommand {
    const val ID = "kobra.preview.open"

    data class Arguments(val uri: String, val index: Int, val label: String?)

    /** The CodeLens's `[{uri, index, label}]`; lsp4j hands them over as Gson trees (or, in tests, maps). */
    fun arguments(command: Command): Arguments? {
        if (command.command != ID) return null
        val first = command.arguments?.firstOrNull() ?: return null
        val json: JsonElement = when (first) {
            is JsonElement -> first
            is Map<*, *> -> JsonObject().apply {
                first.forEach { (k, v) ->
                    when (v) {
                        is Number -> addProperty(k.toString(), v)
                        is String -> addProperty(k.toString(), v)
                        else -> {}
                    }
                }
            }
            is String -> runCatching { JsonParser.parseString(first) }.getOrNull() ?: return null
            else -> return null
        }
        if (json !is JsonObject) return null
        val uri = (json.get("uri") as? JsonPrimitive)?.takeIf { it.isString }?.asString ?: return null
        val index = (json.get("index") as? JsonPrimitive)?.takeIf { it.isNumber }?.asInt ?: 0
        val label = (json.get("label") as? JsonPrimitive)?.takeIf { it.isString }?.asString
        return Arguments(uri, index, label)
    }

    /** Runs the command when it is this one; `false` for another. */
    fun run(project: Project, command: Command): Boolean {
        val args = arguments(command) ?: return false
        KobraPreviewService.getInstance(project).open(args.uri, args.index)
        return true
    }
}

/** The language server's customization: the default, with the preview command run in the IDE. */
class KobraLspCustomization : LspCustomization() {
    override val codeLensCustomizer: LspCodeLensCustomizer = object : LspCodeLensSupport() {
        override fun codeLensClicked(lspClient: LspClient, contextFile: VirtualFile, command: Command, mouseEvent: MouseEvent?) {
            if (!KobraPreviewCommand.run(lspClient.project, command)) {
                super.codeLensClicked(lspClient, contextFile, command, mouseEvent)
            }
        }
    }

    override val commandsCustomizer: LspCommandsCustomizer = object : LspCommandsSupport() {
        override fun executeCommand(lspClient: LspClient, contextFile: VirtualFile, command: Command) {
            if (!KobraPreviewCommand.run(lspClient.project, command)) super.executeCommand(lspClient, contextFile, command)
        }
    }
}
