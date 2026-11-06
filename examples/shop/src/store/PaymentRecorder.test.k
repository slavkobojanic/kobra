import k.crypto.Uuid
import k.server.EmailProvider
import k.server.JobQueue
import k.test.InlineJobQueue
import testing.RecordingMailer

// Integration tests against a real, rolled-back database. InlineJobQueue
// runs each job as soon as the transaction that enqueued it commits, so the
// email is observable within the test.

test PaymentRecorder {
    use InlineJobQueue for JobQueue
    use RecordingMailer for EmailProvider

    @Inject let recorder: PaymentRecorder
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

    test "a matching payment marks the order paid and emails the customer" {
        let order = await placeOrder()

        let outcome = await recorder.recordPayment(orderId: order.id, amount: 48.00.usd, reference: "cs_test_1")
        let stored = await orders.byId(id: order.id)

        expect outcome == .recorded
        expect stored?.status == .paid
        expect stored?.paymentReference == "cs_test_1"
        expect stored?.paidAt == Date.now
        expect mailer.sent.count == 1
        expect mailer.sent[0].to == "ana@example.com"
    }

    test "a repeated delivery changes nothing and sends no second email" {
        let order = await placeOrder()

        await recorder.recordPayment(orderId: order.id, amount: 48.00.usd, reference: "cs_test_1")
        let repeated = await recorder.recordPayment(orderId: order.id, amount: 48.00.usd, reference: "cs_test_1")

        expect repeated == .notAwaitingPayment
        expect mailer.sent.count == 1
    }

    test "a short payment flags the order for review instead of paying it" {
        let order = await placeOrder()

        let outcome = await recorder.recordPayment(orderId: order.id, amount: 24.00.usd, reference: "cs_test_1")

        expect outcome == .amountMismatch(expected: 48.00.usd, received: 24.00.usd)
        expect await orders.byId(id: order.id)?.status == .needsReview
        expect mailer.sent.isEmpty
    }

    test "the right number in the wrong currency is a mismatch too" {
        let order = await placeOrder()

        let outcome = await recorder.recordPayment(orderId: order.id, amount: 48.00.eur, reference: "cs_test_1")

        expect outcome == .amountMismatch(expected: 48.00.usd, received: 48.00.eur)
    }

    test "a payment for a cancelled order is not recorded" {
        let order = await placeOrder()
        await orders.transition(id: order.id, from: .awaitingPayment, to: .cancelled)

        let outcome = await recorder.recordPayment(orderId: order.id, amount: 48.00.usd, reference: "cs_test_1")

        expect outcome == .notAwaitingPayment
        expect await orders.byId(id: order.id)?.status == .cancelled
        expect mailer.sent.isEmpty
    }

    test "a payment for an unknown order is reported, not recorded" {
        let outcome = await recorder.recordPayment(orderId: Uuid.random(), amount: 48.00.usd, reference: "cs_test_1")

        expect outcome == .unknownOrder
        expect mailer.sent.isEmpty
    }
}
