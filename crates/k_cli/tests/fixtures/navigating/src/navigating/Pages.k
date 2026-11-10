/// A long start page: a note field bound to plain state, and a spacer
/// that puts its link to the other page far below the fold.
page Home at "/" {
    @State var note: String = ""

    view {
        heading(level: 1) { "Home" }

        textField(value: bind note)
            .label("Note")

        text { "Note: {note}" }

        paragraph { "The link to the other page is far below." }

        box {}
            .class(.spacer)

        link(to: .about) { "About" }
    }

    style {
        .spacer { height: 300vh; }
    }
}

/// The other page, with a link back.
page About at "/about" {
    view {
        heading(level: 1) { "About" }
        link(to: .home) { "Home" }
    }
}
