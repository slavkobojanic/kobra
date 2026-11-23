/// The frame around every page.
public layout Frame {
    @Slot let content: View

    view {
        heading(level: 1) { "Shop" }

        content
    }
}
