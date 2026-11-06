// Modifier names after `.` may be keywords; `.style(...)` does not enter style mode because it follows a `.`.
component T {
    view {
        box {}
            .style(.compact)
            .default(true)
            .in(group)
    }
}
