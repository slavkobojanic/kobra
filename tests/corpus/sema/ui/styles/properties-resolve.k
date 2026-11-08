// Camel-case property names map 1:1 to CSS; each resolves to its table entry.
component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card {
            padding: 16px;
          //^^^^^^^ resolves: css padding
            borderLeft: 4px solid #3B82F6;
          //^^^^^^^^^^ resolves: css borderLeft
            backgroundColor: #FFFFFF;
            display: flex;
            justifyContent: spaceBetween;
        }
    }
}
