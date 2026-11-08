// CSS keywords are camel case like property names: a kebab-case keyword is K3214
// with a machine-applicable fix to its camel-case form, also inside functions
// and media features. Numbers, colors, tokens and strings are left alone.
component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card {
            textDecoration: line-through;
            justifyContent: space-between;
            transition: opacity 200ms ease-in-out;
            gridTemplateColumns: repeat(auto-fill, 1fr);
            fontFamily: "Inter", sans-serif;
            border: 1px solid #E5E7EB;
            @media (prefersReducedMotion: no-preference) { padding: 4px; }
        }
    }
}
