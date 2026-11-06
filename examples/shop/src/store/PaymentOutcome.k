/// What `PaymentRecorder` did with a confirmed payment.
///
/// Every outcome is final: delivering the same event again can't change it.
/// A webhook therefore acknowledges all of them, and only logs the ones that
/// need a person's attention.
public enum PaymentOutcome implements Equatable {
    /// The order is now paid, and its confirmation email is queued.
    case recorded

    /// The order was no longer awaiting payment, so nothing changed. Usually
    /// this is a repeated delivery of an event that was already recorded; it
    /// also covers a payment that arrives for a cancelled or flagged order.
    case notAwaitingPayment

    /// The amount paid differs from the order total. The order is flagged
    /// `needsReview` and no email is sent.
    case amountMismatch(expected: Money, received: Money)

    /// No order has the id the payment refers to.
    case unknownOrder
}
