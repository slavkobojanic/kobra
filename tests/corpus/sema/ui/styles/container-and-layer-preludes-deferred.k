// `@container` and `@layer` preludes are parsed, but their values are not checked
// in v0 (sema.md §12.5): no diagnostics.
component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        @layer base {
            .card { padding: 4px; }
        }
        @container (minWidth: 400px) {
            .card { padding: 8px; }
        }
    }
}
