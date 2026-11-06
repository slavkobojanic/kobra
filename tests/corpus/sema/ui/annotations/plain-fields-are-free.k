// Fields without annotations may be `let` or `var`; a plain `var` is ordinary,
// non-reactive data (03-reactivity.md), so neither form is reported.
component Notes {
    let title: String = "Notes"
    var scratch: Int = 0
    @Prop let heading: String
    @State var draft: String = ""

    view {
        text { "{title} {heading} {draft}" }
    }
}
