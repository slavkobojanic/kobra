/// What to charge.
public struct ChargeRequest implements Codable {
    /// The amount in cents.
    let cents: Int
    /// The card's token.
    let card: String
}
