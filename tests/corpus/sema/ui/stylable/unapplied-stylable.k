// A `@Stylable` prop that is never applied to an element is a compile error
// (06-styles.md): K3209 at its name.
component Chip {
    @Stylable let container: Classes
    @Stylable let label: Classes

    view {
        text { "chip" }
            .class(label)
    }
}
