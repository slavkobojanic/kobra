// Adding a case to an enum a recorded payload uses is compatible.
import k.crypto.Uuid
import k.server.Job
import k.server.JobError

/// What the confirmation job needs.
struct OrderConfirmation implements Codable {
    let orderId: Uuid
    let speed: Speed
}

/// How fast to send it.
enum Speed implements Codable {
    case normal
    case express
    case overnight
}

/// Sends an order's confirmation.
service OrderConfirmationJob implements Job<OrderConfirmation> {
    func run(payload: OrderConfirmation) async throws JobError {
    }
}
