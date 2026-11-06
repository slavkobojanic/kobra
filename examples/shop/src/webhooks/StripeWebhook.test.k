import k.http.HttpRequest
import k.http.HttpResponse
import k.server.EmailProvider
import k.server.JobQueue
import k.test.InlineJobQueue
import k.test.TestClient
import payments.StripeWebhookConfig
import store.Order
import store.OrderRepository
import store.Product
import store.ProductRepository
import testing.RecordingMailer
import testing.StripeTestEvent

// HTTP-level integration tests: requests go through the real server stack
// (routing, the endpoint page, verification, recording and the job queue)
// in-process, against a rolled-back database.

test StripeWebhook {
    use StripeWebhookConfig.sample for StripeWebhookConfig
    use InlineJobQueue for JobQueue
    use RecordingMailer for EmailProvider

    @Inject let client: TestClient
    @Inject let catalog: ProductRepository
    @Inject let orders: OrderRepository
    @Inject let mailer: RecordingMailer

    /// Inserts a mug and an order for two of them, for $48.00.
    func placeOrder() async -> Order {
        let mug = Product.mug()
        let order = Order.sample(product: mug)

        await catalog.insert(mug)
        await orders.insert(order)

        return order
    }

    /// POSTs a body to the webhook, with a `Stripe-Signature` header when one
    /// is given.
    func deliver(payload: String, signature: String?) async -> HttpResponse {
        let headers: Map<String, String> = if let signature { ["Stripe-Signature": signature] } else { Map() }

        let request = HttpRequest(method: .post, url: Page.stripeWebhook.absoluteUrl)
            .headers(headers)
            .body(.text(payload))

        return await client.send(request: request)
    }

    /// POSTs a correctly signed body to the webhook.
    func deliverSigned(payload: String) async -> HttpResponse {
        return await deliver(payload: payload, signature: StripeTestEvent.signature(payload: payload))
    }

    // Accepted deliveries

    test "a signed payment marks the order paid and is acknowledged" {
        let order = await placeOrder()
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{order.id}", amountTotal: 4800)

        let response = await deliverSigned(payload: payload)

        expect response.status == 204
        expect await orders.byId(id: order.id)?.status == .paid
        expect mailer.sent.count == 1
    }

    test "a repeated delivery is acknowledged without a second email" {
        let order = await placeOrder()
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{order.id}", amountTotal: 4800)

        await deliverSigned(payload: payload)
        let repeated = await deliverSigned(payload: payload)

        expect repeated.status == 204
        expect mailer.sent.count == 1
    }

    test "events the shop ignores are acknowledged so Stripe stops sending them" {
        let order = await placeOrder()
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{order.id}", type: "checkout.session.expired")

        let response = await deliverSigned(payload: payload)

        expect response.status == 204
        expect await orders.byId(id: order.id)?.status == .awaitingPayment
    }

    // Refused deliveries

    test "an unsigned request is refused and changes nothing" {
        let order = await placeOrder()
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{order.id}", amountTotal: 4800)

        let response = await deliver(payload: payload, signature: nil)

        expect response.status == 400
        expect await orders.byId(id: order.id)?.status == .awaitingPayment
        expect mailer.sent.isEmpty
    }

    test "a request signed with another secret is refused" {
        let order = await placeOrder()
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{order.id}", amountTotal: 4800)
        let forged = StripeTestEvent.signature(payload: payload, secret: "whsec_attacker")

        let response = await deliver(payload: payload, signature: forged)

        expect response.status == 400
        expect await orders.byId(id: order.id)?.status == .awaitingPayment
    }

    test "only POST is accepted" {
        let response = await client.send(request: HttpRequest(method: .get, url: Page.stripeWebhook.absoluteUrl))

        expect response.status == 405
        expect response.headers["allow"] == "POST"
    }
}
