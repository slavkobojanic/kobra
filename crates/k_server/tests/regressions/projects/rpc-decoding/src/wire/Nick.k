import k.validation.Length
import k.validation.Validatable

/// A nickname of two or three characters.
public struct Nick implements Codable, Validatable {
    /// Two or three characters (`String.count`, grapheme clusters).
    @Length(min: 2, max: 3, message: "Use 2 to 3 characters")
    var name: String
}
