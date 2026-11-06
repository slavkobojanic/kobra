// A leading `.` continues the chain even across comment lines and blank lines.
component T {
    view {
        box {}
            // explain the class
            .class(.a)

            /* and the event */
            .on(click: f)
    }
}
