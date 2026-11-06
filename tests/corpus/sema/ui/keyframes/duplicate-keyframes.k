// Two keyframes with one name in a block are K3213 at the second name, with a
// label at the first.
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

        @keyframes fade {
            from { opacity: 1; }
            to { opacity: 0; }
        }

        .toast { animationName: .fade; }
    }
}
