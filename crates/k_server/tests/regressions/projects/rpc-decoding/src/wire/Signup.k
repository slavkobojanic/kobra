import k.validation.Email
import k.validation.Matches
import k.validation.Validatable

/// A request with constraints the server must enforce.
public struct Signup implements Codable, Validatable {
    /// Must look like an email address.
    @Email(message: "Enter an email address")
    var contact: String

    /// Lowercase letters, digits and dashes only.
    @Matches("^[a-z0-9-]+$", message: "Use lowercase letters, digits and dashes")
    var handle: String
}
