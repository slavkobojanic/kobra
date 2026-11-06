// Elements without content (`lineBreak`, `image`, inputs, `divider`) take no
// trailing block: each child is K3107.
component Bad {
    @State var name: String = ""

    view {
        lineBreak {
            "x"
        }
        image(source: Url("https://k.dev/a.png"), description: "a") {
            text { "caption" }
        }
        textField(value: bind name) {
            "label"
        }
    }
}
