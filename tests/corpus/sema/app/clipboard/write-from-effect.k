// `Clipboard.write(text:)` and `readText()` are `@UserAction` (45-web-platform.md, "Clipboard"):
// browsers allow them only during a user's action, so calling them from an effect or after an
// `await` is K4030; a click handler may.
import k.device.Clipboard
import k.time.Clock

service Copier {
    @Inject let clipboard: Clipboard
    @Inject let clock: Clock

    @State var problem: Bool = false
    @State var pasted: String = ""

    func copy(_ text: String) async {
        do {
            try await clipboard.write(text: text)
        } catch _ {
            problem = true
        }
    }

    func copyLater(_ text: String) async {
        await clock.sleep(1.seconds)
        do {
            try await clipboard.write(text: text)
        } catch _ {
            problem = true
        }
    }

    func paste() async {
        do {
            pasted = try await clipboard.readText() ?? ""
        } catch _ {
            problem = true
        }
    }
}

component CopyOnLoad {
    @Inject let copier: Copier

    @Effect
    func copyAtOnce() async {
        await copier.copy("hello")
    }

    view {
        button { "Copy" }
            .on(click: { await copier.copy("hello") })
        button { "Paste" }
            .on(click: { await copier.paste() })
    }
}
