// A test file imports the shared fake.
import testing.FakeCart

test CartService {
    use FakeCart for CartService

    @Inject let cart: CartService

    test "starts empty" {
        expect cart.items.count == 0
    }
}
