// `animationName: none` is the CSS keyword, not a keyframes reference.
component Toast {
    view {
        box { "x" }
            .class(.toast)
    }

    style {
        .toast { animationName: none; }
    }
}
