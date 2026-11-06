// ...is visible to every other test file of `shop`.
test "a fixture's cart" {
    let fixture = Fixture(cart: Cart(size: 3))
    //            ^^^^^^^ resolves: decl shop.Fixture
    expect fixture.cart.size == 3
    //     ^^^^^^^ resolves: local fixture 3:9
}
