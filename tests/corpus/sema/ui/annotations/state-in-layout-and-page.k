// `@State` and `@Derived` are allowed in pages and layouts as well as
// components.
page Dashboard {
    @State var tab: Int = 0
    @Derived let isFirst: Bool = tab == 0

    view {
        text { "{tab} {isFirst}" }
    }
}

layout Shell {
    @State var isMenuOpen: Bool = false

    view {
        text { "{isMenuOpen}" }
    }
}
