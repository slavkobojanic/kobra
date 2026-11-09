import k.validation.Length
import k.validation.Range
import k.validation.Validatable

/// What the review form asks for: notes, and a rating.
public struct BookReview implements Codable, Validatable {
    @Length(max: 2000)
    var notes: String

    @Range(0...5)
    var rating: Int
}
