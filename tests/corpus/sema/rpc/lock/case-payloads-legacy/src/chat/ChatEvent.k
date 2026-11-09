/// What the server says.
public enum ChatEvent implements Codable {
    /// The room's recent messages.
    case snapshot(messages: List<Message>, present: List<Member>)
    /// A message was posted: a struct in a payload is still compared through the program's payloads.
    case posted(Message)
}
