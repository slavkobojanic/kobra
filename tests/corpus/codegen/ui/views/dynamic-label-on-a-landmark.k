// A reactive `.label` on a landmark is an `aria-label` binding; on the server it goes through
// `out.attribute`, which escapes (codegen.md §11.2).
component Panel {
    @Prop let title: String

    view {
        section {
            paragraph { "content" }
        }
        .label(title)
    }
}
