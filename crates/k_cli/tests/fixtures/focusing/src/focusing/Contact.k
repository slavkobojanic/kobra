import k.validation.NotBlank
import k.validation.Validatable

/// The contact form's fields.
public struct Contact implements Codable, Equatable, Validatable {
    /// Who is writing.
    @NotBlank(message: "Give your name")
    var name: String

    /// What they write.
    @NotBlank(message: "Write a message")
    var message: String
}
