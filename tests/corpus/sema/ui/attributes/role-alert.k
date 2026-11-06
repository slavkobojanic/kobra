// `.role(.alert)` and `.role(.status)` go on non-interactive elements.
component Banner {
    view {
        paragraph { "Saved" }
            .role(.alert)
            //     ^^^^^ resolves: member k.ui.AriaRole.alert
        box { "Loading" }
            .role(.status)
    }
}
