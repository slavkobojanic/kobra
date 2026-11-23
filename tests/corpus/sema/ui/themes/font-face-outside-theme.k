// A `FontFace` ships only from a theme's font token (07-themes-and-units.md,
// "Shipped fonts"), so one anywhere else is K3311.
component Label {
    let face: FontFace = FontFace("fonts/Inter.woff2")

    view {
        text("x")
    }
}
