import k.server.JobQueue
import k.test.InlineJobQueue
import k.test.RecordingEmailProvider
import k.test.TestClock
import pay.Charge
import pay.Refund
import store.Order
import store.OrderRepository
import store.Product
import store.ProductRepository

// Paybox's events, as its delivery job calls them, in-process. InlineJobQueue
// runs the confirmation job when the payment's transaction commits, so its
// email reaches the default RecordingEmailProvider within the test.

test PaymentEventsApi {
    use InlineJobQueue for JobQueue

    @Inject let events: PaymentEventsApi
    @Inject let catalog: ProductRepository
    @Inject let orders: OrderRepository
    @Inject let mailer: RecordingEmailProvider
    @Inject let clock: TestClock

    /// Inserts a mug and an order for two of them, for $48.00.
    func placeOrder() async throws Product.UniqueViolation -> Order {
        let mug = Product.mug()
        let order = Order.sample(product: mug, placedAt: clock.now)

        try await catalog.insert(mug)
        await orders.insert(order)

        return order
    }

    /// Paybox's charge for `order`, for `amount`.
    func charge(for order: Order, amount: Money) -> Charge {
        return Charge(id: "ch_test_1", reference: "{order.id}", amount: amount, last4: "4242", refunded: 0.usd)
    }

    test "a succeeded charge marks its order paid and emails the customer" {
        let order = try await placeOrder()

        try await events.chargeSucceeded(charge: charge(for: order, amount: 48.00.usd))

        let stored = await orders.byId(order.id)

        expect stored?.status == .paid
        expect stored?.paymentReference == "ch_test_1"
        expect mailer.sent.count == 1
    }

    test "a repeated event is acknowledged without a second email" {
        let order = try await placeOrder()

        try await events.chargeSucceeded(charge: charge(for: order, amount: 48.00.usd))
        try await events.chargeSucceeded(charge: charge(for: order, amount: 48.00.usd))

        expect mailer.sent.count == 1
    }

    test "a charge for a different amount flags the order for review" {
        let order = try await placeOrder()

        try await events.chargeSucceeded(charge: charge(for: order, amount: 24.00.usd))

        expect await orders.byId(order.id)?.status == .needsReview
        expect mailer.sent.isEmpty
    }

    test "a charge for something that isn't an order is acknowledged and changes nothing" {
        let order = try await placeOrder()

        try await events.chargeSucceeded(charge: Charge(id: "ch_test_2", reference: "invoice-7", amount: 48.00.usd, last4: "4242", refunded: 0.usd))

        expect await orders.byId(order.id)?.status == .awaitingPayment
    }

    test "a refund is acknowledged" {
        let order = try await placeOrder()

        try await events.chargeRefunded(refund: Refund(id: "re_test_1", chargeId: "ch_test_1", reference: "{order.id}", amount: 48.00.usd))

        expect await orders.byId(order.id)?.status == .awaitingPayment
    }
}
