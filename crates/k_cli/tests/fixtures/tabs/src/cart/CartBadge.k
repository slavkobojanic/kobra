/// The cart's size, with buttons that change it. The cart lives in the
/// browser, so the component renders there.
@Rendering(.client)
public component CartBadge {
    @Inject let cart: CartService

    loading {
    }

    view {
        text { "Cart {cart.count}" }
        text { "Last {cart.last}" }

        button { "Add a mug" }
            .on(click: { cart.add(name: "Mug", quantity: 1) })

        button { "Empty" }
            .on(click: { cart.clear() })
    }
}
