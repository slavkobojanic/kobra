import k.crypto.Uuid
import payments.CheckoutRequest
import payments.PaymentError
import payments.PaymentGateway

// Integration tests: StoreService calls StoreApi in-process through
// StoreProtocol, against a real database that is rolled back after each test.

test StoreApi {
    use SwitchableGateway for PaymentGateway

    @Inject let store: StoreService
    @Inject let catalog: ProductRepository
    @Inject let orders: OrderRepository
    @Inject let gateway: SwitchableGateway

    func orderMugs(quantity: Int) async throws CheckoutError -> PlacedOrder {
        let request = OrderRequest(productSlug: "test-mug", quantity: quantity, email: "ana@example.com")

        return try await store.placeOrder(request: request)
    }

    func mugStock() async -> Int? {
        return await catalog.bySlug(slug: "test-mug")?.stock
    }

    // Browsing

    test "products are listed with what is available" {
        await catalog.insert(Product.mug(stock: 3))

        let products = await store.products()

        expect products == [ProductSummary.mug.copy(available: 3)]
    }

    test "an unknown slug is not found" {
        expect throws LookupError.notFound {
            try await store.product(slug: "no-such-product")
        }
    }

    // Placing orders

    test "an order reserves stock and waits for payment" {
        await catalog.insert(Product.mug(stock: 5))

        let placed = try await orderMugs(quantity: 2)
        let order = try await store.order(id: placed.orderId)

        expect await mugStock() == 3
        expect order.status == .awaitingPayment
        expect order.total == 48.00.usd
        expect placed.paymentUrl == Page.receipt(id: placed.orderId).absoluteUrl
        expect gateway.requests[0].cancelUrl == Page.productDetail(slug: "test-mug").absoluteUrl
    }

    test "the last units can be ordered" {
        await catalog.insert(Product.mug(stock: 2))

        try await orderMugs(quantity: 2)

        expect await mugStock() == 0
    }

    test "ordering more than is left fails and changes nothing" {
        await catalog.insert(Product.mug(stock: 1))

        expect throws CheckoutError.outOfStock(available: 1) {
            try await orderMugs(quantity: 2)
        }

        expect await mugStock() == 1
        expect await orders.recent(limit: 10).isEmpty
        expect gateway.requests.isEmpty
    }

    test "an unknown product can't be ordered" {
        expect throws CheckoutError.productNotFound {
            try await orderMugs(quantity: 1)
        }
    }

    // When the payment provider fails

    test "an unreachable provider cancels the order and releases its stock" {
        await catalog.insert(Product.mug(stock: 5))
        gateway.simulateFailure(error: .unavailable)

        expect throws CheckoutError.paymentUnavailable {
            try await orderMugs(quantity: 2)
        }

        expect await mugStock() == 5
        expect await orders.recent(limit: 1)[0].status == .cancelled
    }

    test "a refusing provider is reported to the customer the same way" {
        await catalog.insert(Product.mug(stock: 5))
        gateway.simulateFailure(error: .rejected(reason: "Invalid API Key provided"))

        expect throws CheckoutError.paymentUnavailable {
            try await orderMugs(quantity: 2)
        }

        expect await mugStock() == 5
    }

    // Receipts

    test "an unknown order id is not found" {
        expect throws LookupError.notFound {
            try await store.order(id: Uuid.random())
        }
    }
}

/// Behaves like `LocalCheckout` until told to fail, and records every
/// request.
service SwitchableGateway implements PaymentGateway {
    private var failure: PaymentError? = nil

    var requests: List<CheckoutRequest> = []

    func simulateFailure(error: PaymentError) {
        failure = error
    }

    func startCheckout(request: CheckoutRequest) async throws PaymentError -> Url {
        requests.append(request)

        if let failure {
            throw failure
        }

        return request.returnUrl
    }
}
