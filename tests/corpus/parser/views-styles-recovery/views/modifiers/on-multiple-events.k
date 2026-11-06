// Several events in one `.on(...)` call.
component T {
    view {
        row {}
            .on(click: select, doubleClick: open)
    }
}
