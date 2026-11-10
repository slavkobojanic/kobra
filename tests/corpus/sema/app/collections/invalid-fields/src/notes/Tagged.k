import k.crypto.Uuid

/// A value with fields IndexedDB can't key by.
public struct Tagged implements Codable {
    let id: Uuid
    let note: NoteId
    let done: Bool
    let tags: List<String>
    let label: String?
}
