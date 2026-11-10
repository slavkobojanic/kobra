/// The page a push opens.
page Report at "/reports/{id}" {
    @Param let id: String

    view {
        heading(level: 1) { "Report {id}" }
    }
}
