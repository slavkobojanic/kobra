// A helper declared in one test file of `shop`...
struct Fixture {
    let cart: Cart
}

test "fixtures hold a cart" {
    expect Fixture(cart: Cart(size: 2)).cart.size == 2
}
