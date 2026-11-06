import k.crypto.Uuid
import k.db.Database

/// Applies confirmed payments to orders.
///
/// Payment providers deliver webhooks at least once, and sometimes twice at
/// the same moment, so recording is idempotent: marking the order paid is a
/// conditional update that succeeds for exactly one delivery, and the
/// confirmation email is enqueued in the same transaction. The email is sent
/// once per paid order, however many times the event arrives.
public service PaymentRecorder {
    @Inject let orders: OrderRepository
    @Inject let database: Database
    @Inject let confirmations: OrderConfirmationJob

    /// Records that the checkout for an order was paid.
    ///
    /// - Parameters:
    ///   - orderId: the order the checkout was started for.
    ///   - amount: the amount the provider collected.
    ///   - reference: the provider's id for the payment.
    /// - Returns: what happened to the order. See `PaymentOutcome`.
    public func recordPayment(orderId: Uuid, amount: Money, reference: String) async -> PaymentOutcome {
        guard let order = await orders.byId(id: orderId) else {
            return PaymentOutcome.unknownOrder
        }

        guard amount == order.total else {
            let flagged = await orders.transition(id: order.id, from: .awaitingPayment, to: .needsReview)

            if flagged {
                return PaymentOutcome.amountMismatch(expected: order.total, received: amount)
            }

            return PaymentOutcome.notAwaitingPayment
        }

        let recorded = await database.transaction {
            let marked = await orders.markPaid(id: order.id, reference: reference, paidAt: Date.now)

            if marked {
                await confirmations.enqueue(payload: OrderConfirmation(orderId: order.id))
            }

            return marked
        }

        if recorded {
            return PaymentOutcome.recorded
        }

        return PaymentOutcome.notAwaitingPayment
    }
}
