// More HTML names from the 05-views.md table, each one K3101 at its name.
component Contact {
    @State var name: String = ""

    view {
        ul {
            li { "one" }
        }
        input(value: bind name)
        textarea(value: bind name)
        br()
    }
}
