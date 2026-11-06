// With an unknown property there is no expected type for `.md`, but that is not a
// second error (no K2007): one K3201 only.
component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card { gutter: .md; }
    }
}
