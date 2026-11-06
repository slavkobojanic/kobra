// `expect` takes any boolean expression
test "cart" {
    expect cart.items.count == 1
    expect cart.total == 9.99.usd
    expect not cart.isEmpty
    expect screen.text("Total: $9.99").isVisible
}
