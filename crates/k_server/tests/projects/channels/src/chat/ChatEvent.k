/// What the server says on a room's channel.
public enum ChatEvent implements Codable {
    case snapshot(notes: List<String>)
    case posted(text: String)
    case count(n: Int)
    case user(name: String)
}
