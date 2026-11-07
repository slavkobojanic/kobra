// Static text is HTML-escaped by the compiler, then printed as a JS string literal: `&`, `<`, `>`
// become entities, `"` is a JS escape (codegen.md §3, §11.2).
component Note {
    view {
        paragraph { "a < b & \"c\" > d" }
    }
}
