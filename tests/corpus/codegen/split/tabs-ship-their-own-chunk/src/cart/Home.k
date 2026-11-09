public page Home at "/" {
    @Inject let cart: CartService

    view {
        button { "Add" }
            .on(click: { cart.add() })
    }
}
