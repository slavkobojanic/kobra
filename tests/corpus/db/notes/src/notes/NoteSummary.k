import k.crypto.Uuid

/// What clients see of a note.
public struct NoteSummary implements Codable {
    /// The id.
    let id: Uuid

    /// The title.
    let title: String

    /// The text.
    let body: String

    /// The tag labels, sorted.
    let tags: List<String>

    /// Whether it is pinned.
    let isPinned: Bool

    /// Its revision.
    let revision: Int
}
