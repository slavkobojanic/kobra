// `expect throws Pattern { ... }` with qualified and implicit case patterns
test "errors" {
    expect throws CartError.outOfStock {
        try await cart.add(product: .soldOut)
    }
    expect throws .outOfStock(let n) { try f() }
    expect throws _ { }
}
