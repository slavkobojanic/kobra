// `!` in a modifier argument is K0106 and is treated as `not`.
component T {
    view {
        button { "Go" }
            .disabled(!ready)
    }
}
