// A memberwise argument is checked against the field's type.
struct Product {
    let slug: String
    let price: Int
}

let p = Product(slug: "mug", price: "twelve")
