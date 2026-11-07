// A memberwise initializer takes a field's default when it is omitted.
struct Order implements Codable {
    let product: String
    var quantity: Int = 1
    var note: String? = nil
}

func main() -> List<Order> {
    return [Order(product: "mug"), Order(product: "cup", quantity: 3, note: "gift")]
}
