// Sample values shared by tests, as static members added by extensions, so
// tests can write `.sample` or `Product.mug()` wherever a value is expected
// (see docs/design/21-testing.md and testing-support.md).

import admin.AdminConfig
import k.crypto.Uuid
import payments.StripeWebhookConfig
import store.Order
import store.Product
import store.ProductSummary

extend Product {
    /// A $24.00 mug with the slug `test-mug`.
    ///
    /// - Parameter stock: the units in stock.
    public static func mug(stock: Int = 5) -> Product {
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

extend Order {
    /// An order for two units of `product` by `ana@example.com`, awaiting
    /// payment. The total is twice the product's price.
    ///
    /// - Parameter product: the product ordered; insert it first.
    public static func sample(product: Product) -> Order {
        return Order(
            id: Uuid.random(),
            product: product,
            email: "ana@example.com",
            quantity: 2,
            unitPrice: product.price,
            total: product.price * 2
        )
    }
}

extend ProductSummary {
    /// The summary of `Product.mug()`, with 5 units available.
    public static let mug: ProductSummary = ProductSummary(
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

extend AdminConfig {
    /// `admin@shop.test`, with the password `change-me-admin-password`; the
    /// same hash as `secrets/admin_password_hash.txt.example`.
    public static let sample: AdminConfig = AdminConfig(
        email: "admin@shop.test",
        passwordHash: Secret("$argon2id$v=19$m=65536,t=3,p=4$aLYLgjk75OnGzmfOgi+w+g$6DSzMJriTe+IRcfo7YcTibzOSpaPcEMeYxWRXNtBCFY")
    )
}

extend StripeWebhookConfig {
    /// Signs with `StripeTestEvent.secret` and allows the default five
    /// minutes of clock difference.
    public static let sample: StripeWebhookConfig = StripeWebhookConfig(
        signingSecret: Secret(StripeTestEvent.secret),
        tolerance: 5.minutes
    )
}
