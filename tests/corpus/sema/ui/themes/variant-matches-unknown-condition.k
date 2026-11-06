// `matches` takes a `MediaCondition`: `.dark` is not one (K2006); `true` has the
// wrong type (K2001).
theme Tokens {
    color surface: Color = #FFFFFF

    variant dark matches .dark {
        surface = #111827
    }

    variant other matches true {
        surface = #222222
    }
}
