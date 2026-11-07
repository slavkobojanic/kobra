import checkout.Cart

/// The shop's one page: pays for a mug through paybox.
public page Home at "/" {
    @Inject let cart: Cart

    metadata {
        title("Stack shop")
    }

    view {
        main {
            heading(level: 1) { "K mug, $9.90" }

            paragraph { cart.message }

            button { "Pay" }
                .on(click: { await cart.pay() })
        }
    }
}
