/// The page a finished export's notification opens.
page Report at "/reports/{id}" {
    @Param let id: String

    view {
        heading(level: 1) { "Report {id}" }
    }
}
