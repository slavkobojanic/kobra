// `if let footer` unwraps an optional slot by name.
component T {
    view {
        if let footer {
            footer
        }
    }
}
