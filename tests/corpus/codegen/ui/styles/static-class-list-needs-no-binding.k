// A class list without `when` is constant: it is written into the template, with no binding.
component Note {
    view {
        paragraph { "Nothing here yet." }
            .class([.muted, .small])
    }

    style {
        .muted { color: #6B7280; }
        .small { fontSize: 12px; }
    }
}
