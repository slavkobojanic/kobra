import store.Product
import store.ProductRepository

// The storefront end to end: the page loads the catalog through
// StoreService, StoreProtocol in-process and StoreApi, from a database
// rolled back after each test.

test Home {
    @Inject let catalog: ProductRepository

    test "the storefront lists the products with their prices" {
        try await catalog.insert(Product.mug(stock: 3))

        let screen = render(Home())
        await screen.settle()

        expect screen.heading(named: "New this season").isVisible
        expect screen.heading(named: "Test Mug").isVisible
        expect screen.text("$24.00").isVisible
        expect not screen.text("Sold out").exists
    }

    test "a product without stock is marked sold out" {
        try await catalog.insert(Product.mug(stock: 0))

        let screen = render(Home())
        await screen.settle()

        expect screen.text("Sold out").isVisible
    }

    test "an empty catalog says so" {
        let screen = render(Home())
        await screen.settle()

        expect screen.text("The shelves are empty. Check back soon.").isVisible
    }
}
