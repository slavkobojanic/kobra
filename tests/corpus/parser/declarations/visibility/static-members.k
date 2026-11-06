// `static` follows the visibility modifier.
struct Product {
    static let sample = Product(id: 1)
    public static let empty: Product = Product(id: 0)
    private static func make() -> Product {
        return .sample
    }
    static var counter = 0
}
