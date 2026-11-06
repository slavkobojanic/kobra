// A layout declaring a content slot and placing it in its view.
layout Shop {
    @Slot let content: View

    view {
        NavBar()

        main { content }

        Footer()
    }
}
