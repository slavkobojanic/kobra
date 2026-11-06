import k.crypto.Uuid
import k.encoding.Json
import k.http.HttpClient
import k.http.HttpError
import k.http.HttpRequest
import k.http.HttpResponse

test StripeCheckout {
    use StripeApiConfig(apiKey: Secret("sk_test_123")) for StripeApiConfig
    use FakeStripeApi for HttpClient

    @Inject let checkout: StripeCheckout
    @Inject let stripe: FakeStripeApi

    let request = CheckoutRequest(
        orderId: Uuid("33333333-3333-4333-8333-333333333333"),
        customerEmail: "ana@example.com",
        description: "K Mug",
        unitPrice: 24.00.usd,
        quantity: 2,
        returnUrl: Url("https://shop.test/receipt/33333333-3333-4333-8333-333333333333"),
        cancelUrl: Url("https://shop.test/products/k-mug")
    )

    test "a created session's URL is where the customer goes" {
        stripe.respond(status: 200, body: stripe.sessionJson)

        let url = try await checkout.startCheckout(request: request)
        let sent = stripe.requests[0]

        expect url == Url("https://checkout.stripe.com/c/pay/cs_test_1")
        expect sent.url == Url("https://api.stripe.com/v1/checkout/sessions")
        expect sent.headers["Authorization"] == "Bearer sk_test_123"
        expect sent.headers["Idempotency-Key"] == "checkout-33333333-3333-4333-8333-333333333333"
        expect formField(name: "client_reference_id", request: sent) == "33333333-3333-4333-8333-333333333333"
        expect formField(name: "line_items[0][price_data][unit_amount]", request: sent) == "2400"
        expect formField(name: "line_items[0][price_data][currency]", request: sent) == "usd"
        expect formField(name: "line_items[0][quantity]", request: sent) == "2"
    }

    test "a 4xx answer means Stripe refused the request" {
        stripe.respond(status: 401, body: "Invalid API Key provided")

        expect throws PaymentError.rejected {
            try await checkout.startCheckout(request: request)
        }
    }

    test "a 5xx answer means Stripe is unavailable, so the customer can retry" {
        stripe.respond(status: 503, body: "Service Unavailable")

        expect throws PaymentError.unavailable {
            try await checkout.startCheckout(request: request)
        }
    }

    test "a success without a usable session is a refusal, not an outage" {
        stripe.respond(status: 200, body: "<html>maintenance</html>")

        expect throws PaymentError.rejected {
            try await checkout.startCheckout(request: request)
        }
    }
}

/// The value of one form-encoded field in a request's body.
func formField(name: String, request: HttpRequest) -> String? {
    guard let body = request.body else {
        return nil
    }

    return match body {
        case .form(let fields) { fields[name] }

        default { nil }
    }
}

/// Stands in for api.stripe.com: answers every request with one canned
/// response and records what was sent.
service FakeStripeApi implements HttpClient {
    private var response: HttpResponse = HttpResponse(status: 404, body: "No canned response")
    private var failure: HttpError? = nil

    /// Every request sent so far, oldest first.
    var requests: List<HttpRequest> = []

    /// A minimal checkout session, as Stripe returns it.
    let sessionJson: String = Json.encode(CreatedCheckoutSession(id: "cs_test_1", url: Url("https://checkout.stripe.com/c/pay/cs_test_1")))

    func respond(status: Int, body: String) {
        response = HttpResponse(status: status, body: body)
    }

    func fail(error: HttpError) {
        failure = error
    }

    func send(request: HttpRequest) async throws HttpError -> HttpResponse {
        requests.append(request)

        if let failure {
            throw failure
        }

        return response
    }
}
