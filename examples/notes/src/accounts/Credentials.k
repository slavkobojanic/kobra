import k.text.EmailAddress
import k.validation.Format
import k.validation.NotBlank
import k.validation.Validatable

/// The email address and password someone signs in with.
public struct Credentials implements Codable, Validatable {
    /// The account's email address, in any letter case. Its type validates
    /// it: `EmailAddress.parse` trims surrounding spaces, and text that isn't
    /// an address shows the `@Format` message.
    @Format(message: "Enter a valid email address")
    var email: EmailAddress

    /// The account's password, in plain text. It is only ever compared with
    /// the stored hash, and never stored or logged.
    @NotBlank(message: "Enter your password")
    var password: String
}
