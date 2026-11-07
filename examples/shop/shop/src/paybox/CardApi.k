import k.crypto.Uuid
import k.observe.Log
import pay.CardDetails
import pay.ChargeRequest
import pay.DeclineReason
import pay.Payments
import store.OrderRepository

/// The server implementation of `CardProtocol`.
///
/// It charges the order's own total, read from the database, never an
/// amount from the browser. The order's id is the charge's reference, so
/// paying twice for one order charges once: paybox answers the second
/// request with the first charge. The order is marked paid only when
/// paybox's `chargeSucceeded` event arrives, which is the same path a
/// delayed or repeated confirmation takes.
public service CardApi implements CardProtocol {
    @Inject let orders: OrderRepository
    @Inject let payments: Payments

    rpc pay(orderId: Uuid, card: CardForm) async throws CardPaymentError {
        guard let order = await orders.byId(orderId) else {
            throw CardPaymentError.orderNotFound
        }

        guard order.status == .awaitingPayment else {
            throw CardPaymentError.notAwaitingPayment
        }

        let request = ChargeRequest(
            reference: "{order.id}",
            amount: order.total,
            card: CardDetails(number: card.number),
            description: "K Shop: {order.quantity} × {order.product.name}"
        )

        do {
            let charge = try await payments.charge(request: request)

            Log.info("Order {order.id} charged as {charge.id}; waiting for paybox's event")
        } catch .declined(let reason) {
            throw CardPaymentError.declined(message: explain(reason))
        } catch .invalidCard {
            throw CardPaymentError.invalidCard
        } catch .invalidAmount {
            Log.error("Paybox refused the amount {order.total} of order {order.id}")

            throw CardPaymentError.paymentsUnavailable
        } catch .unknownMerchant {
            Log.error("Paybox doesn't know this shop; checkout registers it, so check PAYBOX_URL")

            throw CardPaymentError.paymentsUnavailable
        } catch .rpcFailure(let failure) {
            Log.warning("Paybox is unavailable: {failure}")

            throw CardPaymentError.paymentsUnavailable
        }
    }

    /// What the customer is told about a decline.
    func explain(_ reason: DeclineReason) -> String {
        return match reason {
            case .cardDeclined { "Your card was declined. Try another card." }

            case .insufficientFunds { "Your card has insufficient funds. Try another card." }

            case .expiredCard { "Your card has expired. Try another card." }
        }
    }
}
