// Selectors are built from classes, `&`, pseudo-classes and -elements, attribute
// selectors and `*` (06-styles.md); `@media` nests.
component Card {
    view {
        box { "x" }
            .class([.card, .title])
    }

    style {
        .card {
            padding: 16px;

            &:hover { backgroundColor: #F3F4F6; }

            &::before { opacity: 0.5; }

            &[disabled] { opacity: 0.4; }

            & > * { margin: 0; }

            @media (maxWidth: 600px) { padding: 8px; }
        }

        .card:not(.title) { color: #111111; }

        .title + .title { marginTop: 4px; }
    }
}
