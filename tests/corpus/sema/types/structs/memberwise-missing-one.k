// Omitting a field without a default is K2008, at the gap before `)`.
struct Product {
    let slug: String
    let price: Int
}

let ok = Product(slug: "mug", price: 12)
let bad = Product(slug: "mug")
