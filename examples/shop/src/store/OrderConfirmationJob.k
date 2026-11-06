import k.crypto.Uuid
import k.server.Email
import k.server.EmailProvider
import k.server.Job
import k.server.JobError
import k.server.PublicUrls

/// Emails the customer once their payment is confirmed.
///
/// `PaymentRecorder` enqueues it in the same transaction that marks the order
/// paid, and the worker inside `dist/app` runs it shortly after. SMTP outages
/// are retried with exponential backoff, starting at 30 seconds, for up to 8
/// attempts; a message the server refuses, or one that can't be sent as
/// written, is not retried.
///
/// Delivery is at least once: if the process dies between sending and
/// acknowledging the job, the customer may get the email twice. For an
/// order confirmation, that is the right trade-off.
///
/// The `EmailProvider` is the stdlib's SMTP provider, configured through
/// `SMTP_HOST` and related variables; nothing here depends on which server
/// it talks to. `@JobName` fixes the name its rows have in the queue, so
/// renaming the service never strands queued jobs.
@Retry(attempts: 8, backoff: 30.seconds)
@JobName("order-confirmation")
service OrderConfirmationJob implements Job<OrderConfirmation> {
    @Inject let orders: OrderRepository
    @Inject let mailer: EmailProvider
    @Inject let urls: PublicUrls

    func run(payload: OrderConfirmation) async throws JobError {
        guard let order = await orders.byId(payload.orderId) else {
            throw JobError.permanent(reason: "Order {payload.orderId} no longer exists")
        }

        let email = Email(
            to: order.email,
            subject: "Your K Shop order is confirmed",
            text: confirmationText(for: order)
        )

        do {
            try await mailer.send(email: email)
        } catch .unavailable {
            throw JobError.retryable(reason: "The SMTP server is unavailable")
        } catch .rejected(let reason) {
            throw JobError.permanent(reason: "The SMTP server rejected the message: {reason}")
        } catch .invalidMessage(let reason) {
            throw JobError.permanent(reason: "The message can't be sent as written: {reason}")
        }
    }

    /// The plain-text body of the confirmation email.
    func confirmationText(for order: Order) -> String {
        return [
            "Thanks for your order!",
            "",
            "{order.quantity} × {order.product.name}",
            "Total paid: {order.total.formatted(as: .currency)}",
            "",
            "Your receipt: {urls.absolute(.receipt(id: order.id))}",
            "",
            "We'll email you again when it ships."
        ].joined(separator: "\n")
    }
}

/// The payload of `OrderConfirmationJob`.
///
/// It holds only the order id. The job reads the order when it runs, so the
/// email shows current data, and no personal data sits in the job queue.
struct OrderConfirmation implements Codable, Equatable {
    let orderId: Uuid
}
