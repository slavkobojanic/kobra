// A slot closure whose body starts on the line after the parameter arrow.
component T {
    view {
        Menu()
            .item { entry =>

                MenuItem(entry: entry)

            }
    }
}
