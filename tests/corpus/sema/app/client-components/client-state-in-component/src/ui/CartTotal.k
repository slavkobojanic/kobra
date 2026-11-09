// Has `loading` already: the fix adds only the annotation.
public component CartTotal {
    @Inject let cart: CartService

    view {
        text { "{cart.items.count} items" }
    }

    loading {
        text { "…" }
    }
}
