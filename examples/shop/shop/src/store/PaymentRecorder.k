import k.crypto.Uuid
import k.db.Database
import k.server.JobQueue
import k.time.Clock

/// Applies confirmed payments to orders.
///
/// Payment providers deliver webhooks at least once, and sometimes twice at
/// the same moment, so recording is idempotent: marking the order paid is a
/// conditional update that succeeds for exactly one delivery, and the
/// confirmation email is enqueued in the same transaction. The job becomes
/// visible to the worker only when that transaction commits, so the email is
/// sent once per paid order, however many times the event arrives, and never
/// for a payment that was rolled back.
public service PaymentRecorder {
    @Inject let orders: OrderRepository
    @Inject let database: Database
    @Inject let jobs: JobQueue
    @Inject let clock: Clock

    /// Records that the checkout for an order was paid.
    ///
    /// - Parameters:
    ///   - orderId: the order the checkout was started for.
    ///   - amount: the amount the provider collected.
    ///   - reference: the provider's id for the payment.
    /// - Returns: what happened to the order. See `PaymentOutcome`.
    func recordPayment(orderId: Uuid, amount: Money, reference: String) async -> PaymentOutcome {
        guard let order = await orders.byId(orderId) else {
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
            let marked = await orders.markPaid(id: order.id, reference: reference, paidAt: clock.now)

            if marked {
                await jobs.enqueue(OrderConfirmationJob.kind, payload: OrderConfirmation(orderId: order.id))
            }

            marked
        }

        if recorded {
            return PaymentOutcome.recorded
        }

        return PaymentOutcome.notAwaitingPayment
    }
}
