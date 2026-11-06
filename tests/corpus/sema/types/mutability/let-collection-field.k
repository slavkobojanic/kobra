// A `let` list field can't be mutated through a `var` struct binding.
struct Cart {
    let items: List<String>
}

func add() -> Int {
    var cart = Cart(items: [])
    cart.items.append("mug")
    return cart.items.count
}
