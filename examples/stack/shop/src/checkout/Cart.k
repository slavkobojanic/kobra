/// The browser's side of checkout: calls the shop's own server, which calls
/// paybox. A browser never holds a service token.
public service Cart {
    @Inject let api: CheckoutApi

    /// What the page shows.
    @State var message: String = "Not paid yet"

    /// Pays for the mug.
    func pay() async {
        do {
            let receipt = try await api.pay(orderId: "mug", cents: 990)
            message = "Paid: {receipt.chargeId}"
        } catch CheckoutError.declined {
            message = "Declined"
        } catch CheckoutError.paymentsUnavailable {
            message = "Payments are unavailable; try again"
        } catch CheckoutError.rpcFailure(_) {
            message = "The shop is unreachable; try again"
        }
    }
}
