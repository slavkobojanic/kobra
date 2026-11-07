import k.validation.NotBlank
import k.validation.Validatable

/// A name and a password.
public struct Credentials implements Codable, Validatable {
    @NotBlank(message: "Enter your name")
    var name: String

    @NotBlank(message: "Enter your password")
    var password: String
}
