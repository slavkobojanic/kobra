// Keyframes are hashed like class names: one component can't use another's.
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

        .toast { animationName: .fade; }
    }
}

component Banner {
    view {
        box { "y" }
            .class(.banner)
    }

    style {
        .banner { animationName: .fade; }
    }
}
