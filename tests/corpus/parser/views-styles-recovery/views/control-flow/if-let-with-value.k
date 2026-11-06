// `if let` with an explicit value.
component T {
    view {
        if let user = session.user {
            text { user.name }
        }
    }
}
