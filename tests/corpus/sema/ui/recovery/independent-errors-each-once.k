// Independent mistakes in one component are each reported once and don't affect
// one another.
component Card {
    @State var isOn: Bool = false

    view {
        img(src: "x")
        button { "x" }
            .colour(.red)
            .class(.missing)
        for name in ["a", "b"] {
            text { name }
        }
    }

    style {
        .card { colr: red; }
    }
}
