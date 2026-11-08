// `.primary.mix(.white, 20%)` mixes 20% white into the token's color
// (07-themes-and-units.md, "Colors"): the amount is the share of the color
// mixed in, so the `color-mix()` puts the percentage on `other`. In code the
// same call builds the same text at run time (`$k.colorMix`).
theme Tokens {
    color primary: Color = #3B82F6
}

component Card {
    @Prop let accent: Color = .primary

    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card {
            borderLeft: 4px solid .primary.mix(.white, 20%);
            color: {accent.mix(.black, 30%)};
        }
    }
}
