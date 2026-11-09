/// What the server says.
public enum ChatEvent implements Codable {
    /// A message was posted.
    case posted(text: String)
}
