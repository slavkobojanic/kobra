// Built-in elements compile to their semantic HTML tags; a fully static subtree is one template
// (05-views.md, "Built-in element names"; codegen.md §11.1).
component Article {
    view {
        box {
            paragraph { "Hi" }

            text { "a" }

            emphasis { "b" }

            heading(level: 3) { "T" }

            lineBreak()

            divider()

            codeBlock {
                code { "x" }
            }
        }
    }
}
