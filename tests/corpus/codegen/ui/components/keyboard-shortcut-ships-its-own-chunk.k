// `KeyboardShortcut` is the runtime's keys chunk (runtime.md §12.5): imported from a chunk of its
// own, only by apps with shortcuts; the server renders nothing for it (codegen.md §16).
import k.ui.KeyboardShortcut

component Board {
    @State var cards: Int = 0

    view {
        KeyboardShortcut(key: "n", label: "New card")
            .on(pressed: { cards += 1 })

        text { "{cards} cards" }
    }
}
