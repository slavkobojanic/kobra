// Several percentages separated by commas share one block; `0%` and `100%` are the
// bounds.
component Pulse {
    view {
        box { "x" }
            .class(.pulse)
    }

    style {
        @keyframes pulse {
            0%, 100% { opacity: 1; }
            25%, 75% { opacity: 0.5; }
            50% { opacity: 0; }
        }

        .pulse { animationName: .pulse; animationIterationCount: infinite; }
    }
}
