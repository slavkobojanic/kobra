// `Clipboard` and `Share` are platform interfaces, injectable only into services (K4005); a view
// copies or shares with `CopyButton` and `ShareButton` (45-web-platform.md, "Shapes").
import k.device.Clipboard
import k.device.Share

component Copier {
    @Inject let clipboard: Clipboard
    @Inject let share: Share

    view {
        text { "copy" }
    }
}
