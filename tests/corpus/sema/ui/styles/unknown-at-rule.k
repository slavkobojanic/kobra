// The at-rules are `@media`, `@container`, `@layer` and `@keyframes`; another
// name is K3212 at the name.
component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        @supports (display: grid) {
            .card { display: grid; }
        }
        .card { padding: 4px; }
    }
}
