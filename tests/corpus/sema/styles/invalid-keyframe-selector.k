component Pulse {
    view {
        box { "x" }.class(.pulse)
    }

    style {
        @keyframes pulse {
            from { opacity: 1; }
            middle { opacity: 0.5; }
            150% { opacity: 0; }
            to { opacity: 0; }
        }
        .pulse { animationName: .pulse; }
    }
}
