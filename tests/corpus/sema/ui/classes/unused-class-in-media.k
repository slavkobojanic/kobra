// A class declared only inside `@media` and never referenced is unused (K3204).
component Menu {
    view {
        box { "x" }
            .class(.list)
    }

    style {
        .list { padding: 4px; }

        @media (maxWidth: 600px) {
            .compact { padding: 0; }
        }
    }
}
