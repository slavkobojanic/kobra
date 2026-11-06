// A multi-statement handler lambda spanning several lines.
component T {
    view {
        button { "Reset" }
            .on(click: {
                count = 0
                changed.emit(count)
            })
    }
}
