// The full Card component from the components design: slot props of several types and a view using them.
component Card {
    @Prop let title: String
    @Prop let items: List<CartItem>
    @Slot let content: View
    @Slot let footer: View? = nil
    @Slot let row: (CartItem) -> View

    view {
        article {
            heading(level: 2) { title }

            content

            for item in items key item.id {
                row(item)
            }

            if let footer {
                footer
            }
        }
    }
}
