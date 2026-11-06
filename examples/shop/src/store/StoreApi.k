import k.crypto.Uuid
import k.db.Database
import k.observe.Log
import payments.CheckoutRequest
import payments.PaymentGateway

/// The server implementation of `StoreProtocol`.
///
/// Placing an order is reserve, then pay: stock is reserved and the order
/// recorded in one transaction, and only then is the payment gateway asked
/// for a checkout. The gateway call stays outside the transaction so a slow
/// provider never holds database locks. If the gateway fails, the order is
/// cancelled and its stock released.
///
/// The payment itself is confirmed later and elsewhere: the provider calls
/// the `StripeWebhook` page, which hands the event to `PaymentRecorder`.
public service StoreApi implements StoreProtocol {
    @Inject let catalog: ProductRepository
    @Inject let orders: OrderRepository
    @Inject let database: Database
    @Inject let payments: PaymentGateway

    rpc products() async -> List<ProductSummary> {
        let all = await catalog.all()

        return all.map(summarize)
    }

    rpc product(slug: String) async throws LookupError -> ProductSummary {
        guard let product = await catalog.bySlug(slug: slug) else {
            throw LookupError.notFound
        }

        return summarize(product: product)
    }

    rpc placeOrder(request: OrderRequest) async throws CheckoutError -> PlacedOrder {
        guard let product = await catalog.bySlug(slug: request.productSlug) else {
            throw CheckoutError.productNotFound
        }

        let order = Order(
            id: Uuid.random(),
            product: product,
            email: request.email,
            quantity: request.quantity,
            unitPrice: product.price,
            total: product.price * request.quantity
        )

        try await database.transaction {
            guard await catalog.reserve(id: product.id, quantity: order.quantity) else {
                throw CheckoutError.outOfStock(available: product.stock)
            }

            await orders.insert(order)
        }

        let checkout = CheckoutRequest(
            orderId: order.id,
            customerEmail: order.email,
            description: product.name,
            unitPrice: order.unitPrice,
            quantity: order.quantity,
            returnUrl: Page.receipt(id: order.id).absoluteUrl,
            cancelUrl: Page.productDetail(slug: product.slug).absoluteUrl
        )

        do {
            let paymentUrl = try await payments.startCheckout(request: checkout)

            return PlacedOrder(orderId: order.id, paymentUrl: paymentUrl)
        } catch .unavailable {
            Log.warning("Payment provider unavailable; cancelling order {order.id}")
            await cancel(order: order)

            throw CheckoutError.paymentUnavailable
        } catch .rejected(let reason) {
            Log.error("Payment provider rejected order {order.id}: {reason}")
            await cancel(order: order)

            throw CheckoutError.paymentUnavailable
        }
    }

    rpc order(id: Uuid) async throws LookupError -> OrderSummary {
        guard let order = await orders.byId(id: id) else {
            throw LookupError.notFound
        }

        return order.summary()
    }

    /// Cancels an order whose checkout never started, and releases its stock.
    /// The stock is only released when this call did the cancelling, so it is
    /// never released twice.
    private func cancel(order: Order) async {
        await database.transaction {
            if await orders.transition(id: order.id, from: .awaitingPayment, to: .cancelled) {
                await catalog.release(id: order.product.id, quantity: order.quantity)
            }
        }
    }

    private func summarize(product: Product) -> ProductSummary {
        return ProductSummary(
            id: product.id,
            slug: product.slug,
            name: product.name,
            summary: product.summary,
            description: product.description,
            price: product.price,
            imageUrl: product.imageUrl,
            available: product.stock
        )
    }
}
