// A reactive style value is written as a CSS value of its type, not as interpolation text: an
// `Angle` (degrees) gets `deg` and a `Duration` (milliseconds) `ms`, on the client and in the
// server's `style` attribute alike (06-styles.md, "Reactive values"; codegen.md §12).
component Arrow {
    @Prop let angle: Angle
    @Prop let delay: Duration

    view {
        box { "↑" }
            .class(.arrow)
    }

    style {
        .arrow { rotate: {angle}; animationDelay: {delay}; }
    }
}
