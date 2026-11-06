/// A toast that fades in.
public component Toast {
    view {
        box { "Saved" }
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
