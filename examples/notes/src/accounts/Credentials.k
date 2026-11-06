import k.validation.Email
import k.validation.NotBlank
import k.validation.Validatable

/// The email address and password someone signs in with.
public struct Credentials implements Codable, Validatable {
    /// The account's email address, in any letter case.
    @Email(message: "Enter a valid email address")
    var email: String

    /// The account's password, in plain text. It is only ever compared with
    /// the stored hash, and never stored or logged.
    @NotBlank(message: "Enter your password")
    var password: String
}
