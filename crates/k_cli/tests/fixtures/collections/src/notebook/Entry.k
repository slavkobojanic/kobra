/// One entry of the notebook.
public struct Entry implements Codable, Equatable {
    /// The entry's number, its key.
    let id: Int
    /// What was written.
    let text: String
    /// When it was written; the entries are listed newest first by it.
    let writtenAt: Date
    /// The topic, or `nil`.
    let topic: String?
}
