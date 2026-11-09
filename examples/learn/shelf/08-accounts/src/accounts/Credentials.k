import k.text.EmailAddress
import k.validation.Length
import k.validation.Validatable

/// The email address and password someone signs up or signs in with.
public struct Credentials implements Codable, Validatable {
    /// Its type checks that it's an email address.
    var email: EmailAddress

    @Length(min: 8, message: "Use at least 8 characters")
    var password: String
}
