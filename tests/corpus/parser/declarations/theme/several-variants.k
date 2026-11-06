// A theme may declare several variants; an empty variant is allowed.
theme T {
    color surface: Color = #fff
    color text: Color = #000

    variant dark {
        surface = #111
        text = #eee
    }

    variant highContrast {
        text = #000000
    }

    variant empty {}
}
