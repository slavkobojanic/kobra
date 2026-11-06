// A class list spread over lines with a trailing comma and a compound `when` condition.
component T {
    view {
        box {}
            .class([
                .panel,
                .expanded when isExpanded,
                .dense when items.count > 10,
            ])
    }
}
