/// The layout every page renders inside, with its own metadata default.
public layout SiteLayout {
    @Slot let content: View

    metadata {
        title("SSR Test")
        description("The layout's description.")
    }

    view {
        header {
            text { "site header" }
        }

        main { content }
    }
}
