// A `for` without `in`: K0100 at the token where `in` was expected; the rest of the header still parses.
component T {
    view {
        for item items key item.id {
            row(item)
        }
    }
}
