// Each element of a class list is checked, conditional ones too.
component Card {
    @State var isOn: Bool = false

    view {
        box { "x" }
            .class([.card, .missing, .gone when isOn])
    }

    style {
        .card { padding: 4px; }
    }
}
