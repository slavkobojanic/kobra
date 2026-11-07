/// What to charge.
public struct ChargeRequest implements Codable {
    /// The caller's order this charge pays for.
    let orderId: String
    /// The amount, in cents.
    let cents: Int
}
