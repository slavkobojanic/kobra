/// What to refund.
public struct RefundRequest implements Codable {
    /// The charge to refund, as `Charge.id`.
    let chargeId: String

    /// The amount to give back, in the charge's currency.
    let amount: Money
}
