import k.validation.NotBlank
import k.validation.Validatable

/// The card form on the `Payment` page.
///
/// Paybox is a test provider that decides by card number alone (see
/// `pay.Payments`), so the form asks for nothing else. A shop taking real
/// cards would never handle the number itself: the provider's own page or
/// field would collect it.
public struct CardForm implements Codable, Validatable {
    /// The card number, as typed.
    @NotBlank(message: "Enter your card number.")
    var number: String
}
