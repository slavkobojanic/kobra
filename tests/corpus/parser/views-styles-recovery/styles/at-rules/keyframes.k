// `@keyframes` holds keyframe blocks; a property names the keyframes as an
// implicit member.
component Toast {
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
