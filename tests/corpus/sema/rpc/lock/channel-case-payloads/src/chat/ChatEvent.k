/// What the server says.
public enum ChatEvent implements Codable {
    /// The room's recent messages: a new required field breaks open tabs, which can't decode the case without it.
    case snapshot(messages: List<Message>, present: List<Member>)
    /// A message was posted: the payload's own fields are compared too.
    case posted(Message)
    /// A message was deleted: a removed field breaks.
    case deleted
}
