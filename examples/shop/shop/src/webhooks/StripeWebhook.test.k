import k.http.HttpRequest
import k.http.HttpResponse
import k.server.JobQueue
import k.server.PublicUrls
import k.test.InlineJobQueue
import k.test.RecordingEmailProvider
import k.test.TestClient
import k.test.TestClock
import payments.StripeEvents
import payments.StripeWebhookConfig
import payments.StripeWebhookVerifier
import store.Order
import store.OrderRepository
import store.Product
import store.ProductRepository
import testing.StripeTestEvent

// HTTP-level integration tests: requests go through the real server stack
// (routing, the endpoint page, verification, recording and the job queue)
// in-process, against a rolled-back database. The group enables the
// optional Stripe binding as App.k describes, with the sample secret. InlineJobQueue runs the
// confirmation job when the payment's transaction commits, so its email
// reaches the default RecordingEmailProvider within the test.

test StripeWebhook {
    use StripeWebhookVerifier for StripeEvents
    use StripeWebhookConfig.sample for StripeWebhookConfig
    use InlineJobQueue for JobQueue

    @Inject let client: TestClient
    @Inject let catalog: ProductRepository
    @Inject let orders: OrderRepository
    @Inject let mailer: RecordingEmailProvider
    @Inject let urls: PublicUrls
    @Inject let clock: TestClock

    /// Inserts a mug and an order for two of them, for $48.00.
    func placeOrder() async throws Product.UniqueViolation -> Order {
        let mug = Product.mug()
        let order = Order.sample(product: mug, placedAt: clock.now)

        try await catalog.insert(mug)
        await orders.insert(order)

        return order
    }

    /// POSTs a body to the webhook, with a `Stripe-Signature` header when one
    /// is given.
    func deliver(payload: String, signature: String?) async -> HttpResponse {
        let headers: Map<String, String> = if let signature { ["stripe-signature": signature] } else { [:] }

        let request = HttpRequest(method: .post, url: urls.absolute(endpoint: .stripeWebhook))
            .headers(headers)
            .body(.text(payload))

        return await client.send(request: request)
    }

    /// POSTs a correctly signed body to the webhook.
    func deliverSigned(payload: String) async -> HttpResponse {
        let signature = await StripeTestEvent.signature(payload: payload, signedAt: clock.now)

        return await deliver(payload: payload, signature: signature)
    }

    // Accepted deliveries

    test "a signed payment marks the order paid and is acknowledged" {
        let order = try await placeOrder()
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{order.id}", amountTotal: 4800)

        let response = await deliverSigned(payload: payload)

        expect response.status == .noContent
        expect await orders.byId(order.id)?.status == .paid
        expect mailer.sent.count == 1
    }

    test "a repeated delivery is acknowledged without a second email" {
        let order = try await placeOrder()
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{order.id}", amountTotal: 4800)

        await deliverSigned(payload: payload)
        let repeated = await deliverSigned(payload: payload)

        expect repeated.status == .noContent
        expect mailer.sent.count == 1
    }

    test "events the shop ignores are acknowledged so Stripe stops sending them" {
        let order = try await placeOrder()
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{order.id}", type: "checkout.session.expired")

        let response = await deliverSigned(payload: payload)

        expect response.status == .noContent
        expect await orders.byId(order.id)?.status == .awaitingPayment
    }

    // Refused deliveries

    test "an unsigned request is refused and changes nothing" {
        let order = try await placeOrder()
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{order.id}", amountTotal: 4800)

        let response = await deliver(payload: payload, signature: nil)

        expect response.status == .badRequest
        expect await orders.byId(order.id)?.status == .awaitingPayment
        expect mailer.sent.isEmpty
    }

    test "a request signed with another secret is refused" {
        let order = try await placeOrder()
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{order.id}", amountTotal: 4800)
        let forged = await StripeTestEvent.signature(payload: payload, secret: "whsec_attacker", signedAt: clock.now)

        let response = await deliver(payload: payload, signature: forged)

        expect response.status == .badRequest
        expect await orders.byId(order.id)?.status == .awaitingPayment
    }

    test "only POST is accepted" {
        let response = await client.send(request: HttpRequest(method: .get, url: urls.absolute(endpoint: .stripeWebhook)))

        expect response.status == .methodNotAllowed
        expect response.headers["allow"] == "POST"
    }
}

// Without the optional binding, as App.k has it: the endpoint exists, but
// refuses every delivery before reading it, and no Stripe secret is needed.

test StripeWebhookDisabled {
    use InlineJobQueue for JobQueue

    @Inject let client: TestClient
    @Inject let catalog: ProductRepository
    @Inject let orders: OrderRepository
    @Inject let mailer: RecordingEmailProvider
    @Inject let urls: PublicUrls
    @Inject let clock: TestClock

    test "a delivery, even a correctly signed one, answers 404 and changes nothing" {
        let mug = Product.mug()
        let order = Order.sample(product: mug, placedAt: clock.now)
        try await catalog.insert(mug)
        await orders.insert(order)

        let payload = StripeTestEvent.checkoutCompleted(orderId: "{order.id}", amountTotal: 4800)
        let signature = await StripeTestEvent.signature(payload: payload, signedAt: clock.now)
        let request = HttpRequest(method: .post, url: urls.absolute(endpoint: .stripeWebhook))
            .headers(["stripe-signature": signature])
            .body(.text(payload))

        let response = await client.send(request: request)

        expect response.status == .notFound
        expect await orders.byId(order.id)?.status == .awaitingPayment
        expect mailer.sent.isEmpty
    }
}
