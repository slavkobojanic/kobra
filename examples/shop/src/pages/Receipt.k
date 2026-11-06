import k.crypto.Uuid
import store.OrderSummary
import store.StoreService

/// The receipt for one order, where customers land after paying.
///
/// The URL contains the order's random id, so it works like a private link.
/// Stripe may redirect here a moment before its webhook arrives, which is
/// why an order awaiting payment is shown as being confirmed rather than as
/// unpaid.
public page Receipt {
    /// The order's id.
    @Param let id: Uuid

    @Inject let store: StoreService

    /// The order, loaded before the view renders.
    @Load let order: OrderSummary = try await store.order(id: id)

    /// While the payment is being confirmed, checks again every few seconds,
    /// so the page turns to "Payment received" without a manual refresh. The
    /// effect re-runs when `order.status` changes, which cancels the timer.
    @Effect
    func refreshWhileConfirming() {
        guard order.status == .awaitingPayment else {
            return
        }

        let timer = Timer.every(3.seconds) { reload() }

        onCleanup { timer.cancel() }
    }

    metadata {
        title("Your order · K Shop")
    }

    view {
        heading(level: 1) { "Thanks for your order" }

        definitionList {
            term { "Item" }

            definition { "{order.quantity} × {order.productName}" }

            term { "Total" }

            definition { order.total.formatted(as: .currency) }

            term { "Status" }

            definition { order.status.label }
                .class(.status)
        }

        match order.status {
            case .awaitingPayment {
                paragraph { "We're confirming your payment. A confirmation email will follow shortly." }
            }

            case .paid {
                paragraph { "Payment received. We've emailed a confirmation to {order.email}." }
            }

            case .needsReview, .cancelled {
                paragraph { "Something went wrong with this order. Reply to your confirmation email and we'll sort it out." }
                    .class(.problem)
            }
        }
    }

    loading {
        paragraph { "Loading your order…" }
    }

    failed(let error) {
        match error {
            case .notFound {
                metadata {
                    status(.notFound)
                }

                heading(level: 1) { "Order not found" }

                paragraph { "Check the link in your confirmation email." }
            }
        }
    }

    style {
        .status  { fontWeight: 600; }
        .problem { color: .danger; }
    }
}
