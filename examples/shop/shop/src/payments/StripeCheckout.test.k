import k.crypto.Uuid
import k.http.HttpBody
import k.http.HttpClient
import k.http.HttpError
import k.http.HttpRequest
import k.http.HttpResponse
import k.http.HttpStatus
import k.text.EmailAddress

test StripeCheckout {
    use StripeApiConfig.sample for StripeApiConfig
    use FakeStripeApi for HttpClient

    @Inject let checkout: StripeCheckout
    @Inject let stripe: FakeStripeApi

    let request = CheckoutRequest(
        orderId: Uuid("33333333-3333-4333-8333-333333333333"),
        customerEmail: EmailAddress("ana@example.com"),
        description: "K Mug",
        unitPrice: 24.00.usd,
        quantity: 2,
        returnUrl: Url("https://shop.test/receipt/33333333-3333-4333-8333-333333333333"),
        cancelUrl: Url("https://shop.test/products/k-mug")
    )

    test "a created session's URL is where the customer goes" {
        stripe.respond(status: .ok, body: .json(stripe.session))

        let url = try await checkout.startCheckout(request: request)
        let sent = stripe.requests[0]

        expect url == Url("https://checkout.stripe.com/c/pay/cs_test_1")
        expect sent.url == Url("https://api.stripe.com/v1/checkout/sessions")
        expect sent.bearerToken?.matches("sk_test_123") ?? false
        expect sent.headers["idempotency-key"] == "checkout-33333333-3333-4333-8333-333333333333"
        expect formField("client_reference_id", in: sent) == "33333333-3333-4333-8333-333333333333"
        expect formField("customer_email", in: sent) == "ana@example.com"
        expect formField("line_items[0][price_data][unit_amount]", in: sent) == "2400"
        expect formField("line_items[0][price_data][currency]", in: sent) == "usd"
        expect formField("line_items[0][quantity]", in: sent) == "2"
    }

    test "a 4xx answer means Stripe refused the request" {
        stripe.respond(status: .unauthorized, body: .text("Invalid API Key provided"))

        expect throws PaymentError.rejected {
            try await checkout.startCheckout(request: request)
        }
    }

    test "a 5xx answer means Stripe is unavailable, so the customer can retry" {
        stripe.respond(status: .serviceUnavailable, body: .text("Service Unavailable"))

        expect throws PaymentError.unavailable {
            try await checkout.startCheckout(request: request)
        }
    }

    test "a timeout means Stripe is unavailable too" {
        stripe.fail(error: .timeout)

        expect throws PaymentError.unavailable {
            try await checkout.startCheckout(request: request)
        }
    }

    test "a success without a usable session is a refusal, not an outage" {
        stripe.respond(status: .ok, body: .text("<html>maintenance</html>"))

        expect throws PaymentError.rejected {
            try await checkout.startCheckout(request: request)
        }
    }
}

/// The value of one form-encoded field in a request's body.
func formField(_ name: String, in request: HttpRequest) -> String? {
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
    private var response: HttpResponse = HttpResponse(status: .notFound).body(.text("No canned response"))
    private var failure: HttpError? = nil

    /// Every request sent so far, oldest first.
    var requests: List<HttpRequest> = []

    /// A minimal checkout session, as Stripe returns it.
    let session: CreatedCheckoutSession = CreatedCheckoutSession(id: "cs_test_1", url: Url("https://checkout.stripe.com/c/pay/cs_test_1"))

    func respond(status: HttpStatus, body: HttpBody) {
        response = HttpResponse(status: status).body(body)
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
