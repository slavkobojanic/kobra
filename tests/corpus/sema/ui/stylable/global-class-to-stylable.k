// `global("…")` escapes scoping and is a `Classes` value, so it can be passed to a
// `@Stylable` prop.
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
