// The recorded job is found by its `@JobName` even after the service was
// renamed (the lock still names the old declaration): K8003.
import k.crypto.Uuid
import k.server.Job
import k.server.JobError

/// What the confirmation job needs.
struct OrderConfirmation implements Codable {
    let orderId: Uuid
}

/// Sends an order's confirmation.
@JobName("order-confirmation")
service ConfirmationMailer implements Job<OrderConfirmation> {
    func run(payload: OrderConfirmation) async throws JobError {
    }
}
