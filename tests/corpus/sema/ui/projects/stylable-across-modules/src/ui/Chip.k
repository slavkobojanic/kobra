/// A chip a parent may style through `container`.
public component Chip {
    @Prop let text: String
    @Stylable let container: Classes

    view {
        box { text }
            .class([.chip, container])
    }

    style {
        .chip { padding: 4px; }
    }
}
