// A token declared twice in one theme is K3307 at the second, with a label at the
// first; so is a variant declared twice, and an override repeated in a variant.
theme Tokens {
    color surface: Color = #FFFFFF
    color surface: Color = #EEEEEE

    variant dark {
        surface = #111111
        surface = #222222
    }

    variant dark {
        surface = #333333
    }
}
