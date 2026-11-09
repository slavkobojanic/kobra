// A user-action-only function may be passed by name or as a lambda directly to `.on(…)` or to
// `form.handle(…)`, and nowhere else (45-web-platform.md, "Function values"): stored in a local or a
// field (a prop's default here), or passed to a parameter of function type, the compiler could no
// longer see where it is called, so that is K4030 too.
import k.device.Files

service Importer {
    @Inject let files: Files

    @State var count: Int = 0

    func choose() async {
        do {
            let file = try await files.open(accepting: [])
            count = file.size
        } catch _ {
        }
    }

    func later(_ action: () async -> Void) {
        count = 0
    }

    func schedule() {
        let action = choose
        later(choose)
    }
}

component Picker {
    @Inject let importer: Importer

    @Prop let onPick: () async -> Void = importer.choose

    view {
        button { "Pick" }
            .on(click: importer.choose)
    }
}
