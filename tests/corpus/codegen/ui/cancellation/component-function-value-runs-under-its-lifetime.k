// A component's async function used as a value is called with its K arguments alone and runs under
// the component's lifetime: `$k.own(save, $life)`, which also keeps a stopped call silent for a
// caller that drops its promise (codegen.md §10, runtime.md §3.4).
component Editor {
    @Prop let save: (String) async -> Void

    func submit() async {
        await save("draft")
    }

    view {
        button { "Save" }
            .on(click: submit)
    }
}

component Page {
    @State var last: String = ""

    func keep(text: String) async {
        last = text
    }

    view {
        Editor(save: keep)
    }
}
