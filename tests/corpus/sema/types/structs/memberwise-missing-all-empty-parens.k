// `Product()` with no arguments misses every required field: one K2008 at the gap inside `()`.
struct Product {
    let slug: String
    let price: Int
}

let empty = Product()
