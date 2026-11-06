// A `list` accepts only `item` children: a paragraph or text directly inside is
// K3107 at the child.
component Todo {
    view {
        list {
            item { "ok" }
            paragraph { "no" }
            "loose text"
        }
    }
}
