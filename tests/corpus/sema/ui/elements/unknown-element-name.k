// A lowercase name that is neither an element nor an HTML name nor a member in
// scope is K3101; its attributes and events are not checked, and the class it
// references still counts as used (no K3204).
component Card {
    view {
        panel { "x" }
            .class(.card)
            .on(click: { })
    }

    style {
        .card { padding: 4px; }
    }
}
