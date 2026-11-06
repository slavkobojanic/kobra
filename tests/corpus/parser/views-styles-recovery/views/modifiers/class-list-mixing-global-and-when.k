// A class list mixing a scoped class, a global class and a conditional stylable prop.
component T {
    view {
        box {}
            .class([.row, global("legacy-row"), container, .active when isActive])
    }
}
