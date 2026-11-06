import k.http.HttpClient
import k.http.HttpRequest
import k.observe.Log

/// A `PaymentGateway` backed by Stripe Checkout.
///
/// K has no vendor SDKs, so this calls Stripe's REST API directly:
/// `POST /v1/checkout/sessions`, form-encoded, authenticated with the secret
/// key. The order id goes in `client_reference_id`, which Stripe sends back
/// in the `checkout.session.completed` webhook; it is also the idempotency
/// key, so a retried request never creates a second session for one order.
///
/// The key goes into the request as a `Secret`, through `.bearerToken`, so
/// it is never revealed in this code and is redacted from logs.
public service StripeCheckout implements PaymentGateway {
    @Inject let config: StripeApiConfig
    @Inject let http: HttpClient

    func startCheckout(request: CheckoutRequest) async throws PaymentError -> Url {
        let call = HttpRequest(method: .post, url: config.apiUrl.appending(path: "/v1/checkout/sessions"))
            .bearerToken(config.apiKey)
            .headers(["idempotency-key": "checkout-{request.orderId}"])
            .body(.form([
                "mode": "payment",
                "client_reference_id": "{request.orderId}",
                "customer_email": request.customerEmail.address,
                "success_url": "{request.returnUrl}",
                "cancel_url": "{request.cancelUrl}",
                "line_items[0][quantity]": "{request.quantity}",
                "line_items[0][price_data][currency]": request.unitPrice.currency.code.lowercased(),
                "line_items[0][price_data][unit_amount]": "{request.unitPrice.minorUnits}",
                "line_items[0][price_data][product_data][name]": request.description
            ]))
            .timeout(config.timeout)

        do {
            let session: CreatedCheckoutSession = try await http.fetchJson(request: call)

            return session.url
        } catch .status(let status, let body) where status.isClientError {
            throw PaymentError.rejected(reason: "Stripe answered {status.code}: {body}")
        } catch .decoding(let message) {
            throw PaymentError.rejected(reason: "Unreadable checkout session: {message}")
        } catch let error {
            Log.warning("Stripe is unavailable: {error}")

            throw PaymentError.unavailable
        }
    }
}

/// The part of Stripe's checkout session object this app reads.
struct CreatedCheckoutSession implements Codable {
    let id: String
    let url: Url
}
