// A `font` token prints as the value of the CSS `font` shorthand, size
// then family, with the family quoted as a CSS string and followed by a
// generic fallback stack (sans-serif, or serif and monospace for a family
// whose name says so), so a family that isn't installed never falls back
// to the browser's Times; no `@font-face` is emitted for a font the app
// doesn't ship; a variant's font override prints the same way
// (07-themes-and-units.md, "Fonts"; codegen.md §12).
theme Tokens {
    font body: Font = Font(family: "Inter", size: 16px)
    font heading: Font = Font(size: 1.5rem, family: "Source \"Serif\" 4")
    font code: Font = Font(family: "JetBrains Mono", size: 14px)

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
