// `@keyframes fade` is scoped like a class; `animationName: .fade` resolves to it
// and so does the declared name.
component Toast {
    view {
        box { "Saved" }
            .class(.toast)
    }

    style {
        @keyframes fade {
        //         ^^^^ resolves: keyframes corpus.Toast.fade
            from { opacity: 0; }
            50% { opacity: 0.5; }
            to { opacity: 1; }
        }

        .toast {
            animationName: .fade;
            //              ^^^^ resolves: keyframes corpus.Toast.fade
            animationDuration: 200ms;
        }
    }
}
