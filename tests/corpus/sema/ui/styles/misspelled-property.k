// A misspelled property is K3201 with the closest property as a maybe-incorrect
// fix. Its value, even a bad one, is not checked (no K3202).
component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card { paddng: potato; }
    }
}
