// A preview's body is test code: it imports a `testing` module, binds a
// fake with `use`, injects it, and uses a fixture without an import.
import testing.FakeCart

public component CartBadge {
    @Inject let cart: CartService

    view {
        "{cart.items.count} items"
    }
}

preview "three items" (count: Int = 3) {
    use FakeCart for CartService
    @Inject let cart: FakeCart

    cart.seed(count: count)
    cart.seed(count: Int.sample)

    CartBadge()
}

preview "a helper of the module's tests" {
    TestHost()
}
