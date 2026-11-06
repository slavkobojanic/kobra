// In a style block, a property's K types are the expected types of its implicit members.
public component Card {
    view {
        box { "Card" }
            .class(.card)
    }

    style {
        .card {
            padding: .md;
            //        ^^ resolves: token Spacing.md
            borderRadius: .md;
            //             ^^ resolves: token Radius.md
            backgroundColor: .surface;
            //                ^^^^^^^ resolves: token Color.surface
        }
    }
}
