/// One price from the feed.
public struct Tick implements Codable {
    /// The symbol.
    let symbol: String
    /// The price, in cents.
    let cents: Int
}
