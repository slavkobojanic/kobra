/// A succeeded charge.
public struct Charge implements Codable, Equatable {
    /// Paybox's id for the charge, `ch_` and 32 hexadecimal digits.
    let id: String

    /// The merchant's reference, from the `ChargeRequest`.
    let reference: String

    /// The amount charged.
    let amount: Money

    /// The last four digits of the card, for receipts.
    let last4: String

    /// How much of `amount` has been refunded so far.
    let refunded: Money
}
