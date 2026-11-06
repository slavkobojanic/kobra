// `@Binding`, `@Emits`, `@Slot` and `@Stylable` are component-only: a page has no
// parent to bind, listen, pass content or style it.
page Profile {
    @Binding var name: String
    @Emits let saved: Event<Void>
    @Slot let content: View
    @Stylable let container: Classes
    @State var draft: String = ""

    view {
        text { draft }
    }
}
