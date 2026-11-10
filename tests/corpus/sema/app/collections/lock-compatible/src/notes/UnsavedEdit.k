import k.crypto.Uuid

/// An edit that couldn't be saved.
public struct UnsavedEdit implements Codable {
    let noteId: Uuid
    let title: String
    let savedAt: Date
    let body: String?
}
