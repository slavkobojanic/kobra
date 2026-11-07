// Every dynamic value in server HTML goes through `out.text` or `out.attribute`; nothing dynamic is
// ever passed to `out.raw` (codegen.md §11.2, "There is no raw-HTML construct in K").
component Comment {
    @Prop let author: String
    @Prop let body: String

    view {
        article {
            heading(level: 3) { author }

            paragraph { "{author} wrote: {body}" }
        }
        .label(author)
    }
}
