import k.crypto.Uuid
import k.text.EmailAddress

/// A customer's order for one product, stored in the `order` table.
///
/// Customers check out as guests, so an order carries the email address the
/// receipt goes to rather than an account. The unit price and total are
/// copied from the product when the order is placed, so later price changes
/// never alter what the customer agreed to pay.
public entity Order {
    /// The primary key. Random UUIDs are unguessable, so the receipt URL can
    /// contain the id without exposing other customers' orders.
    @Id let id: Uuid

    /// The product ordered. Load it with `.including { order => order.product }`.
    @Relation let product: Product

    /// Where the receipt and confirmation email go.
    let email: EmailAddress

    /// The number of units ordered.
    let quantity: Int

    /// The product's price when the order was placed.
    let unitPrice: Money

    /// `unitPrice` times `quantity`: the amount the payment must match.
    let total: Money

    /// Where the order is in its lifecycle.
    var status: OrderStatus = .awaitingPayment

    /// The payment provider's id for the completed checkout, kept for refunds
    /// and support. `nil` until the order is paid.
    var paymentReference: String? = nil

    /// When the order was placed. Indexed, because the admin dashboard lists
    /// the newest orders first.
    @Index let placedAt: Date = Date.now

    /// When the payment was confirmed. `nil` until the order is paid.
    var paidAt: Date? = nil

    /// Converts the order into its wire type. The `product` relation must be
    /// loaded.
    func summary() -> OrderSummary {
        return OrderSummary(
            id: id,
            productName: product.name,
            quantity: quantity,
            total: total,
            email: email,
            status: status,
            placedAt: placedAt
        )
    }
}
