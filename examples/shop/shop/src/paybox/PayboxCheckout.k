import k.observe.Log
import k.server.PublicUrls
import pay.Payments
import payments.CheckoutRequest
import payments.PaymentError
import payments.PaymentGateway

/// A `PaymentGateway` backed by paybox, the payment service of this
/// workspace.
///
/// Paybox has no pages, so the shop hosts the card form itself: starting a
/// checkout sends the customer to the shop's `Payment` page for the order,
/// whose form charges the card through `CardApi`. Before that, the shop
/// registers with paybox, giving the address where paybox delivers its
/// `PaymentEvents`. Registering is idempotent, so it happens on every
/// checkout, which keeps paybox's record of the shop current without a
/// separate setup step, and checks that paybox is reachable before the
/// customer types a card number.
///
/// `pay.Payments` is bound with `use remote(PayboxService) for Payments` in
/// `App.k`, so `payments` is a server-only client that signs every call with
/// a service token.
public service PayboxCheckout implements PaymentGateway {
    @Inject let payments: Payments
    @Inject let callback: PayboxCallbackConfig
    @Inject let urls: PublicUrls

    func startCheckout(request: CheckoutRequest) async throws PaymentError -> Url {
        do {
            try await payments.registerMerchant(callbackUrl: callback.url)
        } catch .unknownCaller {
            throw PaymentError.rejected(reason: "Paybox received no service token from the shop")
        } catch .rpcFailure(.rejected) {
            throw PaymentError.rejected(reason: "Paybox refused the shop's service token; check PAYBOX_TOKEN")
        } catch .rpcFailure(let failure) {
            Log.warning("Paybox is unavailable: {failure}")

            throw PaymentError.unavailable
        }

        return urls.absolute(.payment(id: request.orderId))
    }
}
