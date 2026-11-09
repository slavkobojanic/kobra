import k.crypto.Uuid

/// A message as the browser sees it.
public struct ChatMessage implements Codable, Equatable {
    let id: Uuid
    let number: Int
    let author: String
    let text: String
    let sentAt: Date
}
