// `global("btn-primary")` is the class name as written; nothing is emitted for it in the stylesheet
// (06-styles.md, "Scoping").
component Buy {
    view {
        button { "Buy" }
            .class(global("btn-primary"))
    }
}
