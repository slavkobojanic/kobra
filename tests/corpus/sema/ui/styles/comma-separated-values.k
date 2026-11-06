// Commas separate the values of a declaration: a font stack, several animations.
component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card {
            fontFamily: "Inter", sans-serif;
            animationDuration: 200ms, 1s;
            boxShadow: 0 1px 2px #00000033;
        }
    }
}
