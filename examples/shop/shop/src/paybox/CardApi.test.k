import k.test.TestClock
import pay.Payments
import store.Order
import store.OrderRepository
import store.Product
import store.ProductRepository
import testing.FakePaybox

// Paying for an order: CardService calls CardApi in-process through
// CardProtocol, against a rolled-back database, with paybox faked.

test CardApi {
    use FakePaybox for Payments

    @Inject let cards: CardService
    @Inject let paybox: FakePaybox
    @Inject let catalog: ProductRepository
    @Inject let orders: OrderRepository
    @Inject let clock: TestClock

    /// Inserts a mug and an order for two of them, for $48.00.
    func placeOrder() async throws Product.UniqueViolation -> Order {
        let mug = Product.mug()
        let order = Order.sample(product: mug, placedAt: clock.now)

        try await catalog.insert(mug)
        await orders.insert(order)

        return order
    }

    test "the order's own total is charged, with the order id as the reference" {
        let order = try await placeOrder()

        try await cards.pay(orderId: order.id, card: CardForm(number: "4242 4242 4242 4242"))

        expect paybox.charged.count == 1
        expect paybox.charged[0].reference == "{order.id}"
        expect paybox.charged[0].amount == 48.00.usd
        expect paybox.charged[0].description == "K Shop: 2 × Test Mug"
    }

    test "the order stays awaiting payment until paybox's event arrives" {
        let order = try await placeOrder()

        try await cards.pay(orderId: order.id, card: CardForm(number: "4242424242424242"))

        expect await orders.byId(order.id)?.status == .awaitingPayment
    }

    test "a declined card is explained, and the order can still be paid" {
        let order = try await placeOrder()

        expect throws CardPaymentError.declined {
            try await cards.pay(orderId: order.id, card: CardForm(number: "4000 0000 0000 0002"))
        }

        do {
            try await cards.pay(orderId: order.id, card: CardForm(number: "4000000000009995"))
        } catch .declined(let message) {
            expect message == "Your card has insufficient funds. Try another card."
        } catch let other {
            expect "{other}" == "declined"
        }

        try await cards.pay(orderId: order.id, card: CardForm(number: "4242424242424242"))

        expect paybox.charged.count == 1
    }

    test "a number paybox doesn't accept is an invalid card" {
        let order = try await placeOrder()

        expect throws CardPaymentError.invalidCard {
            try await cards.pay(orderId: order.id, card: CardForm(number: "1234"))
        }
    }

    test "an order that isn't awaiting payment isn't charged" {
        let order = try await placeOrder()
        await orders.transition(id: order.id, from: .awaitingPayment, to: .cancelled)

        expect throws CardPaymentError.notAwaitingPayment {
            try await cards.pay(orderId: order.id, card: CardForm(number: "4242424242424242"))
        }

        expect paybox.charged.isEmpty
    }

    test "an unknown order isn't charged" {
        expect throws CardPaymentError.orderNotFound {
            try await cards.pay(orderId: Product.mug().id, card: CardForm(number: "4242424242424242"))
        }
    }

    test "a blank card number is refused by validation before the handler runs" {
        let order = try await placeOrder()

        expect throws CardPaymentError.rpcFailure(.invalid) {
            try await cards.pay(orderId: order.id, card: CardForm(number: " "))
        }

        expect paybox.charged.isEmpty
    }
}

test CardApiOffline {
    @Inject let cards: CardService
    @Inject let catalog: ProductRepository
    @Inject let orders: OrderRepository
    @Inject let clock: TestClock

    test "without a fake, paybox is offline: nothing is charged and the customer can retry" {
        let mug = Product.mug()
        let order = Order.sample(product: mug, placedAt: clock.now)
        try await catalog.insert(mug)
        await orders.insert(order)

        expect throws CardPaymentError.paymentsUnavailable {
            try await cards.pay(orderId: order.id, card: CardForm(number: "4242424242424242"))
        }

        expect await orders.byId(order.id)?.status == .awaitingPayment
    }
}
