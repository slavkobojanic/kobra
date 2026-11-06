// An `@Inject` field of an unknown type reports K1202 once; its uses produce no further errors.
component CartBadge {
    @Inject let cart: CartSrvice

    func clear() {
        cart.items = []
    }

    view {
        text { "{cart.items.count}" }
    }
}
