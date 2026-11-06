// An element attribute's value resolves against the attribute's type from the element table.
component Submit {
    view {
        button { "Send" }
            .type(.submit)
            //     ^^^^^^ resolves: member k.ui.ButtonType.submit
    }
}
