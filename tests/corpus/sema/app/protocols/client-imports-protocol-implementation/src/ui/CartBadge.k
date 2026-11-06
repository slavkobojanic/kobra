// Client code can't import the implementation; the import is reported.
import cart.CartApi

public component CartBadge {
    @Inject let api: CartApi

    view {
        text { "cart" }
    }
}
