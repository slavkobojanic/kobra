// An unreadable storage.lock is K8002 at the offending line.
import k.crypto.Uuid
import k.server.Job
import k.server.JobError

/// What the confirmation job needs.
struct OrderConfirmation implements Codable {
    let orderId: Uuid
}

/// Sends an order's confirmation.
service OrderConfirmationJob implements Job<OrderConfirmation> {
    func run(payload: OrderConfirmation) async throws JobError {
    }
}
