// Classes are scoped per component: under the test profile `.panel` of `Card` is `Card__panel`, and
// the element's `class` attribute uses the scoped name (06-styles.md, "Scoping").
component Card {
    view {
        box { "x" }
            .class(.panel)
    }

    style {
        .panel { padding: 4px; }
    }
}
