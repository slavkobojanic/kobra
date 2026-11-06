import k.crypto.Uuid
import search.Searchable

/// Everything a client shows about one note.
///
/// The wire type for notes: `NotesApi` builds it from a `Note`, because
/// entities never leave the server.
public struct NoteSummary implements Codable, Equatable, Copyable, Searchable {
    /// The note's id, used in `.editNote(id:)` links.
    public let id: Uuid

    /// The note's title.
    public let title: String

    /// The note's full text.
    public let body: String

    /// The tag labels, in alphabetical order.
    public let tags: List<String>

    /// Whether the note sorts before unpinned ones.
    public let isPinned: Bool

    /// The revision this copy shows. Passed back to `update` so the server
    /// can detect an edit made in the meantime.
    public let revision: Int

    /// When the title, body or tags last changed.
    public let updatedAt: Date

    /// The start of the text, for list previews: at most 140 characters,
    /// ending in an ellipsis when shortened.
    public var excerpt: String {
        body.truncated(length: 140, ending: "…")
    }

    /// Searches the title, the text and every tag.
    public func searchableText() -> List<String> {
        return [title, body] + tags
    }

    /// A form draft holding this note's current values, to start an edit.
    public func draft() -> NoteDraft {
        return NoteDraft(title: title, body: body, tags: tags.joined(separator: ", "))
    }
}
