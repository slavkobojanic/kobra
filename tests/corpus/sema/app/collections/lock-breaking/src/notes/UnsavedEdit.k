import k.crypto.Uuid

/// An edit that couldn't be saved.
public struct UnsavedEdit implements Codable {
    let noteId: Uuid
    let savedAt: Date
}
