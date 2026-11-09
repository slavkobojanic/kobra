import k.validation.Length
import k.validation.NotBlank
import k.validation.Validatable

/// What the add-book form asks for, and the rules it must pass.
public struct BookDraft implements Codable, Validatable {
    @NotBlank(message: "Enter the book's title")
    @Length(max: 200)
    var title: String

    @NotBlank(message: "Enter who wrote it")
    @Length(max: 100)
    var author: String
}
