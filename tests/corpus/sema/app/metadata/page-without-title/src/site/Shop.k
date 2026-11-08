/// A layout with a default title for its pages.
public layout Shop {
    @Slot let content: View

    metadata {
        title("Acme Shop")
    }

    view {
        content
    }
}
