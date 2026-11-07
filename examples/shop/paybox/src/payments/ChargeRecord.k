import pay.Charge

/// A succeeded charge, stored in the `charge_record` table.
public entity ChargeRecord {
    /// Paybox's id for the charge, `ch_` and 32 hexadecimal digits; also
    /// `Charge.id`.
    @Id let id: String

    /// `"{merchant}/{reference}"`. Unique, so a merchant's reference is
    /// charged at most once, even when two requests for it race.
    @Unique let key: String

    /// The merchant the charge belongs to.
    @Index let merchant: String

    /// The merchant's reference for the payment.
    let reference: String

    /// The amount charged.
    let amount: Money

    /// The last four digits of the card.
    let last4: String

    /// What the customer paid for.
    let description: String

    /// How much has been refunded so far; never more than `amount`.
    var refunded: Money

    /// When the charge was made.
    let createdAt: Date

    /// The charge as `Payments` returns it.
    func toCharge() -> Charge {
        return Charge(id: id, reference: reference, amount: amount, last4: last4, refunded: refunded)
    }
}
