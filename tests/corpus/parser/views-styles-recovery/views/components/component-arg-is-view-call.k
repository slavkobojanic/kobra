// A component argument whose value is itself an element call with a trailing block.
component T {
    view {
        Tooltip(content: text { "Help" }) {
            Icon(name: "info")
        }
    }
}
