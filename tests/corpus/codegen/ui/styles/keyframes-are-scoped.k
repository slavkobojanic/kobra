// `@keyframes fade` is scoped like a class, and `animationName: .fade` names the scoped keyframes
// (06-styles.md, "Animations").
component Toast {
    view {
        box { "Saved" }
            .class(.toast)
    }

    style {
        @keyframes fade {
            from { opacity: 0; }
            50% { opacity: 0.5; }
            to { opacity: 1; }
        }

        .toast {
            animationName: .fade;
            animationDuration: 200ms;
        }
    }
}
