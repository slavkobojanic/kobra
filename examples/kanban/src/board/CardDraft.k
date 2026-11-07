import k.validation.Length
import k.validation.NotBlank
import k.validation.Validatable

/// What a person types when adding or editing a card: the card editor's
/// `@Form`, validated as they type and again before it is saved.
public struct CardDraft implements Codable, Equatable, Validatable {
    /// The title. Surrounding spaces are removed when the card is saved.
    @NotBlank(message: "Give the card a title")
    @Length(max: 80, message: "Keep the title to 80 characters or fewer")
    var title: String

    /// The notes. May be empty.
    @Length(max: 2000, message: "Keep the notes to 2,000 characters or fewer")
    var notes: String
}
