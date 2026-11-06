// Keyframes declared inside `@media` are keyframes of the block and can be
// referenced.
component Toast {
    view {
        box { "x" }
            .class(.toast)
    }

    style {
        @media (minWidth: 600px) {
            @keyframes slide {
                from { opacity: 0; }
                to { opacity: 1; }
            }
        }

        .toast { animationName: .slide; }
    }
}
