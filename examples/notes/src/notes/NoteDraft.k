import k.validation.Length
import k.validation.NotBlank
import k.validation.Rules
import k.validation.Validatable

/// What a person types when creating or editing a note.
///
/// One struct serves the editor form, the `create` and `update` rpcs and the
/// server's checks, so the rules below run in the browser as the person
/// types and again on the server before `NotesApi` sees the draft.
public struct NoteDraft implements Codable, Equatable, Validatable {
    /// The title. Surrounding spaces are removed when the note is saved.
    @NotBlank(message: "Give the note a title")
    @Length(max: 120, message: "Keep the title under 120 characters")
    public var title: String

    /// The note's text. May be empty.
    @Length(max: 20000, message: "Keep the note under 20,000 characters")
    public var body: String

    /// Comma-separated tags, as typed, for example `work, ideas`.
    @Length(max: 200, message: "Keep the tags under 200 characters")
    public var tags: String

    /// Limits a note to ten distinct tags.
    public func validate(rules: Rules) {
        rules.check(tagLabels().count <= 10, field: .tags, message: "Use at most 10 tags")
    }

    /// The tags as labels: trimmed, lower-cased, without blanks or
    /// duplicates, in the order first written.
    ///
    /// For example, `" Work, ideas,,work "` gives `["work", "ideas"]`.
    public func tagLabels() -> List<String> {
        return tags
            .split(separator: ",")
            .map { label => label.trimmed().lowercased() }
            .filter { label => label.isNotEmpty }
            .distinct()
    }
}
