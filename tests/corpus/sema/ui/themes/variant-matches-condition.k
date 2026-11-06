// `variant dark matches .prefersDark` follows a system preference; the condition
// is a `MediaCondition`.
theme Tokens {
    color surface: Color = #FFFFFF

    variant dark matches .prefersDark {
        surface = #111827
    }

    variant contrast matches .prefersContrast {
        surface = #000000
    }
}
