// `bind` of a nested field path.
component T {
    view {
        TextField(value: bind user.address.city)
    }
}
