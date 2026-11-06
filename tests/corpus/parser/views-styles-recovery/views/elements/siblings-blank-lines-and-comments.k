// Sibling view nodes separated by blank lines, line comments and block comments.
component T {
    view {
        heading(level: 1) { "Title" }


        // the body
        paragraph { "Body" }
        /* a block
           comment */
        footer {}
    }
}
