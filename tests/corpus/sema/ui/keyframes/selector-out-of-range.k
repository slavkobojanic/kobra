// Keyframe selectors are `from`, `to` or `0%`–`100%`: `101%`, `-5%` and a length
// are K3207 at the selector.
component Pulse {
    view {
        box { "x" }
            .class(.pulse)
    }

    style {
        @keyframes pulse {
            from { opacity: 1; }
            101% { opacity: 0; }
            -5% { opacity: 0; }
            50px { opacity: 0; }
            to { opacity: 0; }
        }

        .pulse { animationName: .pulse; }
    }
}
