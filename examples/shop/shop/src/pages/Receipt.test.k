import k.crypto.Uuid
import k.test.TestClock
import store.Order
import store.OrderRepository
import store.Product
import store.ProductRepository

// The receipt a customer lands on after paying: read through the real
// StoreService and StoreApi, and refreshed on the TestClock while the
// payment is being confirmed.

test Receipt {
    @Inject let catalog: ProductRepository
    @Inject let orders: OrderRepository
    @Inject let clock: TestClock

    func placeOrder() async throws Product.UniqueViolation -> Order {
        let mug = Product.mug()
        let order = Order.sample(product: mug, placedAt: clock.now)

        try await catalog.insert(mug)
        await orders.insert(order)

        return order
    }

    test "an order awaiting payment says so, and shows what was bought" {
        let order = try await placeOrder()

        let screen = render(Receipt(id: order.id))
        await screen.settle()

        expect screen.heading(named: "Thanks for your order").isVisible
        expect screen.text("2 × Test Mug").isVisible
        expect screen.text("$48.00").isVisible
        expect screen.text("Awaiting payment").isVisible
    }

    test "the receipt follows the payment while it is being confirmed" {
        let order = try await placeOrder()
        let screen = render(Receipt(id: order.id))
        await screen.settle()

        expect await orders.transition(id: order.id, from: .awaitingPayment, to: .paid)
        clock.advance(by: 3.seconds)
        await screen.settle()

        expect screen.text("Paid").isVisible
        expect screen.text("Payment received. A confirmation email is on its way to ana@example.com.").isVisible
    }

    test "an unknown order is not found" {
        let screen = render(Receipt(id: Uuid("99999999-9999-4999-8999-999999999999")))
        await screen.settle()

        expect screen.heading(named: "Order not found").isVisible
        expect screen.text("Check the link in your confirmation email.").isVisible
    }
}
