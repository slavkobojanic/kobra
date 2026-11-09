/// One message.
public struct Message implements Codable {
    /// The text.
    let text: String
    /// Who wrote it: new and required.
    let author: Int
}
