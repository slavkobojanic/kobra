// Literals must satisfy a data type of the grammar: a length is no `<color>`, a
// duration is no `<length>`.
component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card { color: 16px; width: 200ms; animationDuration: 200ms; }
    }
}
