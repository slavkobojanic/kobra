// Sample values shared by tests, as static members added by public
// extensions, so tests can write `.sample` or `Product.mug()` wherever a value
// is expected. This module is test-only, so the extensions are visible to
// every test file and to no production code (docs/design/21-testing.md).

import admin.AdminConfig
import admin.StaffId
import k.crypto.Uuid
import k.text.EmailAddress
import payments.StripeApiConfig
import payments.StripeWebhookConfig
import store.Order
import store.Product
import store.ProductSummary

public extend Product {
    /// A $24.00 mug with the slug `test-mug`.
    ///
    /// - Parameter stock: the units in stock.
    static func mug(stock: Int = 5) -> Product {
        return Product(
            id: Uuid("11111111-1111-4111-8111-111111111111"),
            slug: "test-mug",
            name: "Test Mug",
            summary: "A mug for tests.",
            description: "Holds exactly one assertion.",
            price: 24.00.usd,
            imageUrl: "https://images.test/mug.jpg",
            stock: stock
        )
    }
}

public extend Order {
    /// An order for two units of `product` by `ana@example.com`, awaiting
    /// payment. The total is twice the product's price.
    ///
    /// - Parameter product: the product ordered; insert it first.
    static func sample(product: Product) -> Order {
        return Order(
            id: Uuid.random(),
            product: product,
            email: EmailAddress("ana@example.com"),
            quantity: 2,
            unitPrice: product.price,
            total: product.price * 2
        )
    }
}

public extend ProductSummary {
    /// The summary of `Product.mug()`, with 5 units available.
    static let mug: ProductSummary = ProductSummary(
        id: Uuid("11111111-1111-4111-8111-111111111111"),
        slug: "test-mug",
        name: "Test Mug",
        summary: "A mug for tests.",
        description: "Holds exactly one assertion.",
        price: 24.00.usd,
        imageUrl: "https://images.test/mug.jpg",
        available: 5
    )
}

public extend StaffId {
    /// The administrator of `AdminConfig.sample`, for tests that sign in
    /// through `SessionManager` rather than the sign-in rpc.
    static let sample: StaffId = StaffId(email: EmailAddress("admin@shop.test"))
}

public extend AdminConfig {
    /// `admin@shop.test`, with the password `change-me-admin-password`. The
    /// hash exists only for tests: the committed
    /// `secrets/admin_password_hash.txt.example` is a placeholder that fails
    /// validation, and `scripts/e2e.sh` generates its own password and hash.
    static let sample: AdminConfig = AdminConfig(
        email: EmailAddress("admin@shop.test"),
        passwordHash: Secret("$argon2id$v=19$m=65536,t=3,p=4$aLYLgjk75OnGzmfOgi+w+g$6DSzMJriTe+IRcfo7YcTibzOSpaPcEMeYxWRXNtBCFY")
    )
}

public extend StripeApiConfig {
    /// The test key `sk_test_123`, against the default API URL and timeout.
    static let sample: StripeApiConfig = StripeApiConfig(apiKey: Secret("sk_test_123"))
}

public extend StripeWebhookConfig {
    /// Signs with `StripeTestEvent.secret` and allows the default five
    /// minutes of clock difference.
    static let sample: StripeWebhookConfig = StripeWebhookConfig(
        signingSecret: Secret(StripeTestEvent.secret),
        tolerance: 5.minutes
    )
}
