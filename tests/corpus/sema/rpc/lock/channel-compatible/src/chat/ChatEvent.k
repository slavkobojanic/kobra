/// What the server says: a removed case is compatible, since the server stops sending it.
public enum ChatEvent implements Codable {
    /// A message was posted.
    case posted(Message)
    /// The room's recent messages.
    case snapshot(messages: List<Message>)
    /// A message was edited.
    case edited(Message)
}
