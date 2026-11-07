import k.text.EmailAddress
import k.validation.Format
import k.validation.NotBlank
import k.validation.Range
import k.validation.Validatable

/// What a customer submits from the product page to place an order.
///
/// The same struct backs the `@Form` on `ProductDetail`, travels as the
/// `placeOrder` request, and is validated again on the server before
/// `StoreApi` sees it, so the constraints below are enforced even for
/// requests that never came from the form.
public struct OrderRequest implements Codable, Equatable, Validatable {
    /// The product to order, by slug.
    @NotBlank var productSlug: String

    /// How many units to order. Capped so a single order can't empty the
    /// stock of a small shop.
    @Range(1...10, message: "You can order between 1 and 10 at a time.")
    var quantity: Int

    /// Where the receipt and confirmation email go. The type validates the
    /// address; `@Format` sets the message shown when it doesn't parse.
    @Format(message: "Enter the email address for your receipt.")
    var email: EmailAddress
}
