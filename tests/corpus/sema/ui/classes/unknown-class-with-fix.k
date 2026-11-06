// A misspelled class is K3203 with the closest class as a fix; `.title` is not
// counted as used by the misspelling, so it is also unused (K3204).
component Card {
    view {
        heading(level: 2) { "x" }
            .class(.titel)
    }

    style {
        .title { fontWeight: bold; }
    }
}
