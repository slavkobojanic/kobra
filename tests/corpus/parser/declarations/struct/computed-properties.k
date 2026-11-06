// A field with a block is a computed property.
struct Cart {
    var items: List<Item>
    var total: Money {
        items.sum { item => item.price }
    }
    var isEmpty: Bool { items.isEmpty }
    static var empty: Cart { Cart(items: []) }
}
