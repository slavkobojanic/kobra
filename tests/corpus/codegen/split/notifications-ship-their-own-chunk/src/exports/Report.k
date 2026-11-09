public page Report at "/reports/{id}" {
    @Param let id: String

    view {
        text { "Report {id}" }
    }
}
