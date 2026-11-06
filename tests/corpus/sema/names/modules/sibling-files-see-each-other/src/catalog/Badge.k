// A package-private helper that refers back to the public declaration of a sibling file.
struct Badge {
    let text: String

    func matches(product: Product) -> Bool {
    //                    ^^^^^^^ resolves: decl catalog.Product
        return product.name == text
    }
}
