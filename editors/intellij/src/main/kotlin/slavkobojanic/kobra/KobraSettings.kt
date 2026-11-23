package slavkobojanic.kobra

import com.intellij.openapi.components.BaseState
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.SimplePersistentStateComponent
import com.intellij.openapi.components.State
import com.intellij.openapi.components.Storage
import com.intellij.openapi.components.service
import com.intellij.openapi.options.BoundConfigurable
import com.intellij.openapi.project.ProjectManager
import com.intellij.openapi.ui.DialogPanel
import com.intellij.ui.dsl.builder.AlignX
import com.intellij.ui.dsl.builder.bindSelected
import com.intellij.ui.dsl.builder.bindText
import com.intellij.ui.dsl.builder.panel
import slavkobojanic.kobra.preview.KobraPreviewService

/**
 * The plugin's settings: the `k` executable, like the VS Code extension's
 * `k.server.path`, and whether the preview panel opens by itself.
 */
@Service(Service.Level.APP)
@State(name = "KobraSettings", storages = [Storage("kobra.xml")])
class KobraSettings : SimplePersistentStateComponent<KobraSettings.Options>(Options()) {
    class Options : BaseState() {
        /** Empty: `k` on `PATH`, then `~/.k/bin/k`. */
        var serverPath by string("")

        /** Show the Kobra Preview tool window when a file with previews is selected. */
        var autoOpenPreview by property(true)
    }

    var serverPath: String
        get() = state.serverPath.orEmpty()
        set(value) {
            state.serverPath = value.trim()
        }

    var autoOpenPreview: Boolean
        get() = state.autoOpenPreview
        set(value) {
            state.autoOpenPreview = value
        }

    companion object {
        fun getInstance(): KobraSettings = service()
    }
}

/** Settings | Languages & Frameworks | Kobra. */
class KobraConfigurable : BoundConfigurable("Kobra") {
    override fun createPanel(): DialogPanel {
        val settings = KobraSettings.getInstance()
        return panel {
            row("Path to k:") {
                textField()
                    .align(AlignX.FILL)
                    .bindText(settings::serverPath)
                    .comment(
                        "The <code>k</code> executable; the language server runs as <code>k lsp</code>. " +
                            "Empty: <code>k</code> on <code>PATH</code>, then <code>~/.k/bin/k</code>. " +
                            "A bare name is looked up on <code>PATH</code>.",
                    )
            }
            row {
                checkBox("Open the Kobra Preview tool window for files with previews")
                    .bindSelected(settings::autoOpenPreview)
                    .comment(
                        "Selecting a <code>.k</code> file with a <code>preview</code> shows its previews " +
                            "beside the editor (<code>k preview --editor</code>).",
                    )
            }
        }
    }

    override fun apply() {
        super.apply()
        for (project in ProjectManager.getInstance().openProjects) {
            KobraLspIntegrationProvider.restart(project)
            KobraPreviewService.getInstance(project).restartAll()
        }
    }
}
