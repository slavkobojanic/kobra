/// One message: a new optional field is compatible.
public struct Message implements Codable {
    /// The text.
    let text: String
    /// Who wrote it.
    let author: Int
    /// Whether it was edited.
    let edited: Bool?
}
