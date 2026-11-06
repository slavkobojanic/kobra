import k.crypto.Uuid
import k.http.HttpRequest
import k.test.Screen
import k.test.TestClient
import store.CheckoutError
import store.LookupError
import store.OrderRequest
import store.OrderSummary
import store.PlacedOrder
import store.ProductSummary
import store.StoreService

test ProductDetail {
    use FakeStoreService for StoreService

    @Inject let store: FakeStoreService
    @Inject let client: TestClient

    /// Renders the mug's page, waits for it to load and fills in the form.
    func renderAndFill(email: String = "ana@example.com") async -> Screen {
        let screen = render(ProductDetail(slug: "test-mug"))
        await screen.settle()

        screen.textField(labeled: "Email for your receipt").fill(email)

        return screen
    }

    // Rendering

    test "the page shows the product and an order form" {
        let screen = render(ProductDetail(slug: "test-mug"))
        await screen.settle()

        expect screen.heading(named: "Test Mug").isVisible
        expect screen.text("$24.00").isVisible
        expect screen.button(named: "Buy now").isVisible
    }

    test "a sold-out product has no order form" {
        store.setAvailable(available: 0)

        let screen = render(ProductDetail(slug: "test-mug"))
        await screen.settle()

        expect screen.text("Sold out").isVisible
        expect screen.button(named: "Buy now").exists == false
    }

    test "an unknown product shows a way back to the shop" {
        let screen = render(ProductDetail(slug: "no-such-product"))
        await screen.settle()

        expect screen.heading(named: "Product not found").isVisible
        expect screen.link(named: "Back to the shop").isVisible
    }

    // Ordering

    test "submitting the form places the order" {
        let screen = await renderAndFill()

        screen.button(named: "Buy now").click()
        await screen.settle()

        expect store.placedOrders == [OrderRequest(productSlug: "test-mug", quantity: 1, email: "ana@example.com")]
    }

    test "an invalid email is caught before anything is sent" {
        let screen = await renderAndFill(email: "not-an-email")

        screen.button(named: "Buy now").click()
        await screen.settle()

        expect screen.text("Enter the email address for your receipt.").isVisible
        expect store.placedOrders.isEmpty
    }

    test "running out of stock at checkout says how many are left" {
        store.simulateCheckoutFailure(error: .outOfStock(available: 1))
        let screen = await renderAndFill()

        screen.button(named: "Buy now").click()
        await screen.settle()

        expect screen.text("Only 1 left. Lower the quantity and try again.").isVisible
    }

    test "a rate-limited checkout asks the customer to wait" {
        store.simulateCheckoutFailure(error: .rejected(.rateLimited(retryAfter: 30.seconds)))
        let screen = await renderAndFill()

        screen.button(named: "Buy now").click()
        await screen.settle()

        expect screen.text("Too many orders from your connection. Please wait a minute and try again.").isVisible
    }

    // What crawlers see

    test "the server-rendered page has a canonical link and product data" {
        let url = Page.productDetail(slug: "test-mug").absoluteUrl

        let response = await client.send(request: HttpRequest(method: .get, url: url))

        expect response.status == 200
        expect response.body.contains("<title>Test Mug · K Shop</title>")
        expect response.body.contains("<link rel=\"canonical\" href=\"{url}\">")
        expect response.body.contains("<script type=\"application/ld+json\">")
        expect response.body.contains("\"@type\":\"Product\"")
    }
}

/// A `StoreService` that knows one product, `ProductSummary.mug`, and
/// records orders instead of placing them.
service FakeStoreService implements StoreService {
    private var mug: ProductSummary = .mug
    private var checkoutFailure: CheckoutError? = nil

    /// Every order request received, oldest first.
    var placedOrders: List<OrderRequest> = []

    func setAvailable(available: Int) {
        mug = ProductSummary.mug.copy(available: available)
    }

    func simulateCheckoutFailure(error: CheckoutError) {
        checkoutFailure = error
    }

    public func products() async -> List<ProductSummary> {
        return [mug]
    }

    public func product(slug: String) async throws LookupError -> ProductSummary {
        guard slug == mug.slug else {
            throw LookupError.notFound
        }

        return mug
    }

    public func placeOrder(request: OrderRequest) async throws CheckoutError -> PlacedOrder {
        if let checkoutFailure {
            throw checkoutFailure
        }

        placedOrders.append(request)

        let orderId = Uuid.random()

        return PlacedOrder(orderId: orderId, paymentUrl: Page.receipt(id: orderId).absoluteUrl)
    }

    public func order(id: Uuid) async throws LookupError -> OrderSummary {
        throw LookupError.notFound
    }
}
