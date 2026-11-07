import k.validation.Length
import k.validation.Range
import k.validation.Rules
import k.validation.Validatable

/// A review sent to `addReview`: validated on the server before the handler.
public struct ReviewDraft implements Codable, Validatable {
    /// The review text.
    @Length(min: 3, message: "Write at least 3 characters")
    var text: String

    /// One to five stars.
    @Range(1...5, message: "Choose 1 to 5 stars")
    var stars: Int

    /// Must not repeat the text.
    var title: String

    /// A cross-field rule.
    func validate(rules: Rules) {
        rules.check(title != text, field: .title, message: "The title must differ from the text")
    }
}
