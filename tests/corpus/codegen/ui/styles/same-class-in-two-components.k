// The same class name in two components never collides (06-styles.md, "Scoping").
component Left {
    view {
        box { "l" }
            .class(.panel)
    }

    style {
        .panel { color: #FF0000; }
    }
}

component Right {
    view {
        box { "r" }
            .class(.panel)
    }

    style {
        .panel { color: #0000FF; }
    }
}
