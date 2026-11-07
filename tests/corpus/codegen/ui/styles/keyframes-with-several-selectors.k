// Several percentages share one keyframe block (06-styles.md).
component Pulse {
    view {
        box { "x" }
            .class(.pulse)
    }

    style {
        @keyframes pulse {
            0%, 100% { opacity: 1; }
            50% { opacity: 0; }
        }

        .pulse { animationName: .pulse; animationIterationCount: infinite; }
    }
}
