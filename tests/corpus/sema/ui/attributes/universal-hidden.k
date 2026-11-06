// `.hidden(Bool)` and `.class(...)` are universal: every element has them.
component Panel {
    @State var isHidden: Bool = false

    view {
        box { "x" }
            .hidden(isHidden)
        lineBreak()
            .hidden(true)
        textField(value: bind name)
            .hidden(isHidden)
    }

    @State var name: String = ""
}
