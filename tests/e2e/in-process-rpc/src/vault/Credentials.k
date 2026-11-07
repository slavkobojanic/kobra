import k.validation.NotBlank
import k.validation.Validatable

/// A name.
public struct Credentials implements Codable, Validatable {
    @NotBlank(message: "Enter your name")
    var name: String
}
