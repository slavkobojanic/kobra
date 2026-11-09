/// One message: a new required field breaks both directions.
public struct Message implements Codable {
    /// The text.
    let text: String
    /// Who wrote it.
    let author: Int
    /// The room.
    let room: Int
}
