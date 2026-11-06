// An interpolation containing an if expression, whose blocks nest braces inside the interpolation.
component T {
    style {
        .a {
            opacity: {if isVisible { 1 } else { 0 }};
        }
    }
}
