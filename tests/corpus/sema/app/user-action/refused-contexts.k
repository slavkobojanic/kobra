// Outside a user's action a `@UserAction` member, or a function that calls one first, is K4030:
// in an `@Effect`, a `@Load` initializer, `init()`, a timer callback, a `for await` body, a handler
// of an event that isn't a user's action (`focus`, or a component event not declared
// `@UserAction`), and after an `await`, when the user's action may have expired. The note names the
// path to the `@UserAction` member.
import k.device.File
import k.device.FileOpenError
import k.device.Files
import k.time.Clock

interface Prompter {
    @UserAction
    func prompt()
}

service SilentPrompter implements Prompter {
    func prompt() {
    }
}

service Importer {
    @Inject let files: Files
    @Inject let clock: Clock
    @Inject let prompter: Prompter

    @State var text: String = ""

    init() {
        prompter.prompt()
        clock.after(1.seconds) {
            prompter.prompt()
        }
    }

    func choose() async throws FileOpenError -> File {
        return try await files.open(accepting: [])
    }

    func chooseCount() async -> Int {
        do {
            return try await files.openAll(accepting: []).count
        } catch _ {
            return 0
        }
    }

    func importLater() async {
        await refresh()
        do {
            text = try await files.open(accepting: []).name
        } catch _ {
            text = ""
        }
    }

    func refresh() async {
        text = ""
    }

    func follow(names: Stream<String>) async {
        for await name in names {
            text = name
            do {
                let file = try await choose()
                text = file.name
            } catch _ {
            }
        }
    }
}

component Panel {
    @Emits let closed: Event<Void>

    view {
        button { "Close" }
            .on(click: { closed.emit() })
    }
}

component Picker {
    @Inject let importer: Importer

    @State var name: String = ""

    @Load let first: Int = await importer.chooseCount()

    @Effect
    func start() async {
        do {
            let file = try await importer.choose()
            name = file.name
        } catch _ {
        }
    }

    func pick() async {
        do {
            let file = try await importer.choose()
            name = file.name
        } catch _ {
        }
    }

    loading {
        text { "Loading" }
    }

    view {
        button { "Pick" }
            .on(focus: { await pick() })
        Panel()
            .on(closed: pick)
    }
}
