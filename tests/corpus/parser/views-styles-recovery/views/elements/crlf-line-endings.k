// A view written with CRLF line endings parses exactly like one with LF endings.
component T {
    view {
        box {
            text { "a" }

            text { "b" }
        }
            .class(.wide)
    }
}
