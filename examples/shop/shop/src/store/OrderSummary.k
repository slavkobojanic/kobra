import k.crypto.Uuid
import k.text.EmailAddress

/// An order as the browser sees it, on the receipt page and in the admin
/// table.
public struct OrderSummary implements Codable, Equatable {
    /// The order's id.
    let id: Uuid

    /// The name of the product ordered.
    let productName: String

    /// The number of units ordered.
    let quantity: Int

    /// The amount charged.
    let total: Money

    /// Where the receipt and confirmation email go.
    let email: EmailAddress

    /// Where the order is in its lifecycle.
    let status: OrderStatus

    /// When the order was placed.
    let placedAt: Date
}
