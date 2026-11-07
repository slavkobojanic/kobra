/// One chat message: a struct element is its own message on the wire.
public struct Message implements Codable {
    let room: String
    let text: String
}
