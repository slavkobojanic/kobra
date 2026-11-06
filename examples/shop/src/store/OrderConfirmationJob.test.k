import k.crypto.Uuid
import k.server.JobError
import k.server.PublicUrls
import k.test.RecordingEmailProvider
import k.test.TestClock
import k.text.EmailAddress

// The job is run directly, as the worker would. Email goes to the default
// RecordingEmailProvider, so nothing is sent.

test OrderConfirmationJob {
    @Inject let job: OrderConfirmationJob
    @Inject let mailer: RecordingEmailProvider
    @Inject let catalog: ProductRepository
    @Inject let orders: OrderRepository
    @Inject let urls: PublicUrls
    @Inject let clock: TestClock

    /// Inserts a mug and an order for two of them.
    func placeOrder() async throws Product.UniqueViolation -> Order {
        let mug = Product.mug()
        let order = Order.sample(product: mug, placedAt: clock.now)

        try await catalog.insert(mug)
        await orders.insert(order)

        return order
    }

    test "the email names the item, the total and the receipt link" {
        let order = try await placeOrder()

        try await job.run(payload: OrderConfirmation(orderId: order.id))

        let email = mailer.sent[0]

        expect email.to == EmailAddress("ana@example.com")
        expect email.subject == "Your K Shop order is confirmed"
        expect email.text.contains("2 × Test Mug")
        expect email.text.contains("Total paid: $48.00")
        expect email.text.contains("{urls.absolute(.receipt(id: order.id))}")
    }

    test "an SMTP outage is retried later" {
        let order = try await placeOrder()
        mailer.simulateFailure(error: .unavailable)

        expect throws JobError.retryable {
            try await job.run(payload: OrderConfirmation(orderId: order.id))
        }
    }

    test "a message the server refuses is not retried" {
        let order = try await placeOrder()
        mailer.simulateFailure(error: .rejected(reason: "550 mailbox unavailable"))

        expect throws JobError.permanent {
            try await job.run(payload: OrderConfirmation(orderId: order.id))
        }
    }

    test "a message that can't be sent as written is not retried" {
        let order = try await placeOrder()
        mailer.simulateFailure(error: .invalidMessage(reason: "line break in the subject"))

        expect throws JobError.permanent {
            try await job.run(payload: OrderConfirmation(orderId: order.id))
        }
    }

    test "a job for an order that no longer exists is not retried" {
        expect throws JobError.permanent {
            try await job.run(payload: OrderConfirmation(orderId: Uuid.random()))
        }

        expect mailer.sent.isEmpty
    }
}
