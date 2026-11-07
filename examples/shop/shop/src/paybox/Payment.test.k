import k.test.TestClock
import store.Order
import store.OrderRepository
import store.Product
import store.ProductRepository

// The card form a customer pays on, read through the real StoreService and
// StoreApi.

test Payment {
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

    test "an order awaiting payment shows its total and the card form" {
        let order = try await placeOrder()

        let screen = render(Payment(id: order.id))
        await screen.settle()

        expect screen.heading(named: "Pay for your order").isVisible
        expect screen.text("2 × Test Mug, $48.00").isVisible
        expect screen.textField(labeled: "Card number").isVisible
        expect screen.button(named: "Pay $48.00").isVisible
    }

    test "a paid order links to its receipt instead" {
        let order = try await placeOrder()
        await orders.transition(id: order.id, from: .awaitingPayment, to: .paid)

        let screen = render(Payment(id: order.id))
        await screen.settle()

        expect screen.text("This order is paid.").isVisible
        expect screen.link(named: "See your receipt").isVisible
    }
}
