/// A refund of part or all of a charge.
public struct Refund implements Codable, Equatable {
    /// Paybox's id for the refund, `re_` and 32 hexadecimal digits.
    let id: String

    /// The charge refunded.
    let chargeId: String

    /// The merchant's reference of that charge.
    let reference: String

    /// The amount given back by this refund.
    let amount: Money
}
