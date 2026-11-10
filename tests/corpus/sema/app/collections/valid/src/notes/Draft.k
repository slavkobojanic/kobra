import k.crypto.Uuid

/// A draft kept in the browser.
public struct Draft implements Codable {
    let id: Uuid
    let noteId: Uuid?
    let title: String
    let savedAt: Date
    let words: Int
    let mood: Mood
}
