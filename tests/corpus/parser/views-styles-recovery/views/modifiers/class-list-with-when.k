// A class list with a conditional class written with `when`.
component T {
    view {
        box {}
            .class([.panel, .expanded when isExpanded])
    }
}
