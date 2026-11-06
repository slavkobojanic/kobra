component Toast {
    view {
        box { "Saved" }.class(.toast)
    }

    style {
        @keyframes fade {
            from { opacity: 0; }
            to { opacity: 1; }
        }
        @keyframes spin {
            from { opacity: 1; }
            to { opacity: 0; }
        }
        .toast {
            animationName: .fade;
            animationDuration: 200ms;
            &:hover { animationName: .missing; }
        }
    }
}
