// A `variant` overrides existing tokens with values of the same type; the
// override's name resolves to the token.
theme Tokens {
    color surface: Color = #FFFFFF
    color text: Color = #111111

    variant dark {
        surface = #111827
      //^^^^^^^ resolves: token Color.surface
        text = #F9FAFB
    }
}
