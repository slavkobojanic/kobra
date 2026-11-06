// The global class escape hatch `global("...")` is an ordinary call argument.
component T {
    view {
        button { "Buy" }
            .class(global("btn-primary"))
    }
}
