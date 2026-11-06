// Likewise `.class(.fade)` with only keyframes named `fade` is K3203, and the
// keyframes stay unused.
component Toast {
    view {
        box { "x" }
            .class(.fade)
    }

    style {
        @keyframes fade {
            from { opacity: 0; }
            to { opacity: 1; }
        }
    }
}
