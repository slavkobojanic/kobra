// `.class` takes `Classes`: a `String` prop is K2001; a `Classes` prop
// (`@Stylable`) is fine.
component Chip {
    @Prop let name: String
    @Stylable let container: Classes

    view {
        box { "x" }
            .class(name)
        box { "y" }
            .class(container)
    }
}
