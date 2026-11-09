// No `loading` block: the fix inserts one.
public component CartBadge {
    @Inject let cart: CartService

    view {
        text { "{cart.items.count}" }
    }
}
