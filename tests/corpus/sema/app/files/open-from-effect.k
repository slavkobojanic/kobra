// `Files.open(accepting:)` and `openAll(accepting:)` are `@UserAction` (42-files.md, "Files";
// 45-web-platform.md, "User activation"): browsers show the picker only during a user's action, so
// opening it from an effect is K4030.
import k.device.Files

service Restorer {
    @Inject let files: Files

    @State var text: String = ""

    func restore() async {
        do {
            let file = try await files.openAll(accepting: [.json])
            text = "{file.count} files"
        } catch _ {
            text = ""
        }
    }
}

component RestorePrompt {
    @Inject let restorer: Restorer

    @Effect
    func offer() async {
        await restorer.restore()
    }

    view {
        text { restorer.text }
    }
}
