// `let` in a view is a local constant, readable by the nodes after it.
struct Product {
    let name: String
    let price: Int
}

component Summary {
    @Prop let product: Product

    view {
        let label = "{product.name}: {product.price}"
        //  ^^^^^ type: String

        paragraph { label }
        //          ^^^^^ resolves: local label 11:13
    }
}
