// A component in another module uses the app's tokens. `fontSize: .md` finds no
// `FontSize` token named `md` (only `Spacing` and `Radius` have one): K3202.
public component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card { padding: .md; borderRadius: .md; fontSize: .md; }
    }
}
