/// The fixture's start page, rendered by the server.
page Home at "/" {
    view {
        heading(level: 1) { "Offline fixture" }

        paragraph { "Rendered by the server." }

        UpdateBanner()

        AskServer()
    }
}
