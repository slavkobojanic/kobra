// The shape of every type reachable from the payload is recorded: removing a
// field of a nested struct is incompatible too (K8003).
import k.crypto.Uuid
import k.server.Job
import k.server.JobError

/// What the confirmation job needs.
struct OrderConfirmation implements Codable {
    let orderId: Uuid
    let address: Address
}

/// Where to send it.
struct Address implements Codable {
    let city: String
}

/// Sends an order's confirmation.
service OrderConfirmationJob implements Job<OrderConfirmation> {
    func run(payload: OrderConfirmation) async throws JobError {
    }
}
