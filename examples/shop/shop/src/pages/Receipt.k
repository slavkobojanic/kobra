import k.crypto.Uuid
import k.time.Clock
import store.OrderSummary
import store.StoreService

/// The receipt for one order, where customers land after paying.
///
/// The URL contains the order's random id, so it works like a private link.
/// The card form sends the customer here as soon as paybox accepts the
/// charge, usually a moment before paybox's `PaymentEvents` call marks the
/// order paid, which is why an order awaiting payment is shown as being
/// confirmed rather than as unpaid.
public page Receipt {
    /// The order's id.
    @Param let id: Uuid

    @Inject let store: StoreService
    @Inject let clock: Clock

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

        let timer = clock.every(3.seconds) { reload() }

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
                paragraph { "Payment received. A confirmation email is on its way to {order.email}." }
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
                    title("Order not found · K Shop")
                    status(.notFound)
                }

                heading(level: 1) { "Order not found" }

                paragraph { "Check the link in your confirmation email." }
            }

            case .rpcFailure {
                heading(level: 1) { "Your order couldn't be loaded" }

                paragraph { "Check your connection and try again." }

                button { "Try again" }
                    .on(click: reload)
            }
        }
    }

    style {
        .status  { fontWeight: 600; }
        .problem { color: .danger; }
    }
}
