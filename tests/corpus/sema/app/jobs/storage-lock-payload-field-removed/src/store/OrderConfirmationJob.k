// storage.lock records the job's payload with a `note` field; removing it
// means queued jobs can't be read by this release (30-background-jobs.md,
// "Stored payloads"): K8003 at the payload type argument.
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
