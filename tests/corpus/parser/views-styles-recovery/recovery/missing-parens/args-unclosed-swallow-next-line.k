// Newlines are insignificant inside `(`, so a modifier on the next line becomes part of the unclosed argument.
component T {
    view {
        button(label: "Save"
            .on(click: save)
    }
}
