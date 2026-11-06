// `.class(.muted)` refers to a class of this component's style block; the class
// selector's name and the reference resolve to the same class.
component Summary {
    view {
        paragraph { "Nothing here yet." }
            .class(.muted)
            //      ^^^^^ resolves: class corpus.Summary.muted
    }

    style {
        .muted { color: #6B7280; }
       //^^^^^ resolves: class corpus.Summary.muted
    }
}
