import pay.Refund

/// A refund of a charge, stored in the `refund_record` table.
public entity RefundRecord {
    /// Paybox's id for the refund, `re_` and 32 hexadecimal digits; also
    /// `Refund.id`.
    @Id let id: String

    /// The charge refunded.
    @Index let chargeId: String

    /// The merchant the charge belongs to.
    let merchant: String

    /// The merchant's reference of the charge.
    let reference: String

    /// The amount given back.
    let amount: Money

    /// When the refund was made.
    let createdAt: Date

    /// The refund as `Payments` returns it.
    func toRefund() -> Refund {
        return Refund(id: id, chargeId: chargeId, reference: reference, amount: amount)
    }
}
