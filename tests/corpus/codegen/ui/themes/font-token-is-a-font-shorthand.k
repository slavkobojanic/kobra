// A `font` token prints as the value of the CSS `font` shorthand, size
// then family, with the family quoted as a CSS string; a variant's font
// override prints the same way (07-themes-and-units.md, codegen.md §12).
theme Tokens {
    font body: Font = Font(family: "Inter", size: 16px)
    font heading: Font = Font(size: 1.5rem, family: "Source \"Serif\" 4")

    variant large {
        body = Font(family: "Inter", size: 20px)
    }
}

component Article {
    view {
        box { "x" }
            .class(.article)
    }

    style {
        .article { font: .body; }
    }
}
