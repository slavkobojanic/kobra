// The slots example of 04-components.md: a required trailing slot, an optional
// chained slot and a scoped slot.
struct CartItem {
    let id: Int
    let name: String
}

component CartRow {
    @Prop let item: CartItem

    view {
        text { item.name }
    }
}

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

component CartPage {
    @State var items: List<CartItem> = []

    func checkout() {
    }

    view {
        Card(title: "Your cart", items: items) {
            paragraph { "Review before checkout." }
        }
        .row { item =>
            CartRow(item: item)
        }
        .footer {
            button { "Checkout" }
                .on(click: checkout)
        }
    }
}
