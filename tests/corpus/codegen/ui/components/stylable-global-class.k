// `global("…")` passed to a `@Stylable` prop is the class name as written (06-styles.md).
component Chip {
    @Stylable let container: Classes

    view {
        text { "chip" }
            .class(container)
    }
}

component Home {
    view {
        Chip()
            .container(global("btn-primary"))
    }
}
