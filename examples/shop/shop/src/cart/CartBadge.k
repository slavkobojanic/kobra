/// The cart in the header: how many products it holds, and a button that
/// empties it.
///
/// The cart lives in the browser, so the component is rendered there; the
/// server renders its empty `loading` block at the end of the header.
@Rendering(.client)
public component CartBadge {
    @Inject let cart: CartService

    view {
        box {
            text { summary() }

            if not cart.items.isEmpty {
                button { "Empty cart" }
                    .on(click: { cart.clear() })
                    .class(.empty)
            }
        }
        .class(.cart)
    }

    /// "Cart: 1 item", "Cart: 3 items".
    private func summary() -> String {
        return if cart.items.count == 1 { "Cart: 1 item" } else { "Cart: {cart.items.count} items" }
    }

    loading {
    }

    style {
        .cart {
            display: flex;
            gap: .sm;
            alignItems: center;
        }

        .empty {
            padding: .sm;
            borderRadius: .md;
        }
    }
}
