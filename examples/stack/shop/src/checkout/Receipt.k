/// What the browser shows after paying.
public struct Receipt implements Codable {
    /// Paybox's charge id.
    let chargeId: String
    /// The amount, in cents.
    let cents: Int
}
