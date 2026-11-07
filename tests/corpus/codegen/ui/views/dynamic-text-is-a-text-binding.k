// A dynamic text is a one-space text node in the template and one render effect; the server
// renderer sends it through `out.text`, which escapes (codegen.md §11.1, §11.2).
component Name {
    @Prop let name: String

    view {
        paragraph { name }
    }
}
