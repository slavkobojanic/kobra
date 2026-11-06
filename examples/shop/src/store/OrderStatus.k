/// Where an order is in its lifecycle.
///
/// ```
/// awaitingPayment ──▶ paid
///        │
///        ├──▶ needsReview   the provider reported a different amount
///        └──▶ cancelled     checkout could not be started
/// ```
///
/// Every transition is a conditional update from one expected status (see
/// `OrderRepository.transition(id:from:to:)`), so a repeated or concurrent
/// request can never apply the same transition twice. Fulfilment (packing
/// and shipping) is out of scope for this example.
public enum OrderStatus implements Codable, Equatable {
    /// Stock is reserved and the customer has been sent to pay.
    case awaitingPayment

    /// The payment provider confirmed the full amount.
    case paid

    /// The payment provider reported an amount that doesn't match the order
    /// total. Someone has to look at it before it ships.
    case needsReview

    /// Checkout never started, and the reserved stock was released.
    case cancelled

    /// A short label for receipts and the admin table.
    public var label: String {
        match self {
            case .awaitingPayment { "Awaiting payment" }

            case .paid { "Paid" }

            case .needsReview { "Needs review" }

            case .cancelled { "Cancelled" }
        }
    }
}
