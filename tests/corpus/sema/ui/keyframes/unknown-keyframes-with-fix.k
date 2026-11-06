// A misspelled keyframes name is K3205 with the closest keyframes as a fix; the
// keyframes are then unused (K3206).
component Toast {
    view {
        box { "x" }
            .class(.toast)
    }

    style {
        @keyframes fade {
            from { opacity: 0; }
            to { opacity: 1; }
        }

        .toast { animationName: .fdae; }
    }
}
