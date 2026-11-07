// `@Breaking("reason")` on the job accepts an incompatible payload change.
import k.crypto.Uuid
import k.server.Job
import k.server.JobError

/// What the confirmation job needs.
struct OrderConfirmation implements Codable {
    let orderId: Uuid
}

/// Sends an order's confirmation.
@Breaking("the queue is drained before this release")
service OrderConfirmationJob implements Job<OrderConfirmation> {
    func run(payload: OrderConfirmation) async throws JobError {
    }
}
