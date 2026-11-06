// A `@Stylable` prop is an option: setting it twice is K3104; to pass several
// classes, use a list.
component Chip {
    @Stylable let container: Classes

    view {
        text { "chip" }
            .class(container)
    }
}

component Home {
    @State var isActive: Bool = false

    view {
        Chip()
            .container(.wide)
            .container(.active)
        Chip()
            .container([.wide, .active when isActive])
    }

    style {
        .wide { width: 100%; }
        .active { color: #FF0000; }
    }
}
