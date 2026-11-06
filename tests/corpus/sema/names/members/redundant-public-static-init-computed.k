// path: src/shop/Price.k
// K1302 applies to every member kind: static members, initializers and computed properties.
public struct Price {
    let cents: Int

    public static let zero = Price(cents: 0)

    public init(cents: Int) {
        self.cents = cents
    }

    public var isFree: Bool {
        cents == 0
    }
}
