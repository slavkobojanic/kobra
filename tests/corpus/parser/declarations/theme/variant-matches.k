// `variant name matches .condition` follows an operating system preference.
theme T {
    color surface: Color = #fff

    variant dark matches .prefersDark {
        surface = #111
    }
    variant contrast matches .prefersContrast {}
}
