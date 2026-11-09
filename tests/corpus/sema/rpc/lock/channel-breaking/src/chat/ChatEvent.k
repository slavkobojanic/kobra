/// What the server says.
public enum ChatEvent implements Codable {
    /// A message was posted.
    case posted(Message)
    /// The sender is typing.
    case typing
}
