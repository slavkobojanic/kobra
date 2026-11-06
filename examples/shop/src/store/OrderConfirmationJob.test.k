import k.crypto.Uuid
import k.server.EmailProvider
import k.server.JobError
import testing.RecordingMailer

test OrderConfirmationJob {
    use RecordingMailer for EmailProvider

    @Inject let job: OrderConfirmationJob
    @Inject let mailer: RecordingMailer
    @Inject let catalog: ProductRepository
    @Inject let orders: OrderRepository

    /// Inserts a mug and an order for two of them.
    func placeOrder() async -> Order {
        let mug = Product.mug()
        let order = Order.sample(product: mug)

        await catalog.insert(mug)
        await orders.insert(order)

        return order
    }

    test "the email names the item, the total and the receipt link" {
        let order = await placeOrder()

        try await job.run(payload: OrderConfirmation(orderId: order.id))

        let email = mailer.sent[0]

        expect email.to == "ana@example.com"
        expect email.subject == "Your K Shop order is confirmed"
        expect email.text.contains("2 × Test Mug")
        expect email.text.contains("Total paid: $48.00")
        expect email.text.contains("{Page.receipt(id: order.id).absoluteUrl}")
    }

    test "an SMTP outage is retried later" {
        let order = await placeOrder()
        mailer.simulateFailure(error: .unavailable)

        expect throws JobError.retryable {
            try await job.run(payload: OrderConfirmation(orderId: order.id))
        }
    }

    test "a message the server refuses is not retried" {
        let order = await placeOrder()
        mailer.simulateFailure(error: .rejected(reason: "550 mailbox unavailable"))

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
