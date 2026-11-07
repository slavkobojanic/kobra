// Reaches the server-only implementation through a service.
public page CartPage {
    @Inject let cart: Cart

    view {
        text { "cart" }
    }
}
