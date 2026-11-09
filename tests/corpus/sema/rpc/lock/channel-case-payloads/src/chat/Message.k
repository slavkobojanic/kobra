/// One message: carried only inside a case's payload.
public struct Message implements Codable {
    /// The text.
    let text: String
    /// Who wrote it.
    let author: Int
}
