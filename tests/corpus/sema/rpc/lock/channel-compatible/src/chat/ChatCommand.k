/// What a client says: a new case is compatible.
public enum ChatCommand implements Codable {
    /// Posts a message.
    case post(text: String)
    /// The sender is typing.
    case typing
    /// Edits the sender's last message.
    case edit(text: String)
}
