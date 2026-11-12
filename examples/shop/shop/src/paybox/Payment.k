import k.crypto.Uuid
import k.validation.Form
import k.validation.ValidationError
import store.OrderSummary
import store.StoreService

/// Where a customer pays for an order with a card, after placing it.
///
/// `PayboxCheckout` sends the customer here. Paying sends them on to the
/// `Receipt`, which shows the order as being confirmed until paybox's
/// event arrives a moment later. Like the receipt, the URL contains the
/// order's random id, so it works like a private link.
public page Payment {
    /// The order's id.
    @Param let id: Uuid

    @Inject let store: StoreService
    @Inject let cards: CardService

    /// The order, loaded before the view renders.
    @Load let order: OrderSummary = try await store.order(id: id)

    /// The card form, starting blank.
    @Form var card: CardForm = .empty
    @State var problem: String? = nil

    /// Pays and goes to the receipt. Validation failures reported by the
    /// server are rethrown, so the form shows them under the field. After a
    /// decline the form keeps the page, so the customer can try another
    /// card.
    func pay() async throws ValidationError {
        problem = nil

        do {
            try await cards.pay(orderId: id, card: card.value)

            Router.navigate(to: .receipt(id: id))
        } catch .declined(let message) {
            problem = message
        } catch .invalidCard {
            problem = "That isn't a valid card number. Check it and try again."
        } catch .notAwaitingPayment {
            Router.navigate(to: .receipt(id: id))
        } catch .orderNotFound {
            problem = "This order no longer exists."
        } catch .paymentsUnavailable {
            problem = "Payments are unavailable right now. You haven't been charged; please try again in a minute."
        } catch .rpcFailure(.invalid(let failure)) {
            throw failure
        } catch .rpcFailure(.rejected) {
            problem = "Too many attempts. Please wait a minute and try again."
        } catch .rpcFailure(.offline) {
            problem = "You seem to be offline. Check your connection and try again."
        } catch .rpcFailure(.timeout) {
            problem = "The shop didn't answer in time. Check your receipt before paying again."
        }
    }

    metadata {
        title("Pay for your order · K Shop")
    }

    view {
        heading(level: 1) { "Pay for your order" }

        paragraph { "{order.quantity} × {order.productName}, {order.total.formatted(as: .currency)}" }

        if order.status == .awaitingPayment {
            form {
                textField(value: bind card.number)
                    .label("Card number")

                paragraph { "This shop pays through paybox, a test provider: use 4242 4242 4242 4242 to pay, or 4000 0000 0000 0002 to see a decline." }
                    .class(.hint)

                if let problem {
                    paragraph { problem }
                        .class(.problem)
                        .role(.alert)
                }

                button { "Pay {order.total.formatted(as: .currency)}" }
                    .type(.submit)
                    .disabled(card.isSubmitting)
            }
            .class(.form)
            .on(submit: card.handle(pay))
        } else {
            paragraph { "This order is {order.status.label.lowercased()}." }

            link(to: .receipt(id: id)) { "See your receipt" }
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

                link(to: .home) { "Back to the shop" }
            }

            case .rpcFailure {
                heading(level: 1) { "Your order couldn't be loaded" }

                button { "Try again" }
                    .on(click: reload)
            }
        }
    }

    style {
        .form { display: grid; gap: .md; maxWidth: 20rem; }
        .hint { color: .muted; fontSize: 0.875rem; }
        .problem { color: .danger; }
    }
}
