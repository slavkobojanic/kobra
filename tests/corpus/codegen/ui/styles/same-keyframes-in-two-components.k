// Keyframes named alike in two components don't collide (06-styles.md).
component Toast {
    view {
        box { "t" }
            .class(.toast)
    }

    style {
        @keyframes fade {
            from { opacity: 0; }
            to { opacity: 1; }
        }

        .toast { animationName: .fade; }
    }
}

component Banner {
    view {
        box { "b" }
            .class(.banner)
    }

    style {
        @keyframes fade {
            from { opacity: 1; }
            to { opacity: 0; }
        }

        .banner { animationName: .fade; }
    }
}
