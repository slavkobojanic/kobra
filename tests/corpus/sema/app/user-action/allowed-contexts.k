// A member marked `@UserAction` (here `Files.open`) may be called from the synchronous part of a
// user's action (45-web-platform.md, "User activation"): a click handler passed by name or as a
// lambda, a `submit` handler, the action passed to `form.handle(…)`, and a handler of a component
// event declared `@UserAction @Emits`, such as `KeyboardShortcut.pressed`. The property is inferred
// through app functions: `Importer.choose()` calls `open` before its first `await`, so it is
// user-action-only too, and so is `Toolbar.importFile()`, which calls it. The `await` on the call
// itself is fine, and so is any `await` after it.
import k.device.File
import k.device.FileOpenError
import k.device.Files
import k.ui.KeyboardShortcut
import k.validation.Validatable

service Importer {
    @Inject let files: Files

    @State var names: List<String> = []

    func choose() async throws FileOpenError -> File {
        let file = try await files.open(accepting: [])
        names.append(file.name)
        return file
    }

    func chooseMany() async {
        do {
            let chosen = try await files.openAll(accepting: [.json])
            for file in chosen {
                names.append(file.name)
            }
        } catch _ {
            names = []
        }
    }
}

struct Upload implements Validatable {
    let title: String
}

// A button whose `pressed` event is a user's action: it fires only from a click.
component ImportButton {
    @UserAction
    @Emits let pressed: Event<Void>

    view {
        button { "Import" }
            .on(click: { pressed.emit() })
    }
}

component Toolbar {
    @Inject let importer: Importer

    @Form var upload: Upload = Upload(title: "")

    @State var last: String = ""

    func importFile() async {
        do {
            let file = try await importer.choose()
            last = file.name
        } catch _ {
            last = ""
        }
    }

    func save() async {
        await importer.chooseMany()
    }

    view {
        button { "Import" }
            .on(click: importFile)
        button { "Import many" }
            .on(click: { await importer.chooseMany() })
        form {
            textField(value: bind upload.title)
                .label("Title")
        }
            .on(submit: upload.handle(save))
        ImportButton()
            .on(pressed: importFile)
        KeyboardShortcut(key: "o", label: "Import")
            .on(pressed: { await importer.chooseMany() })
    }
}
