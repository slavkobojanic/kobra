/// What a client says.
public enum ChatCommand implements Codable {
    /// Posts a message: the lock has no fields to compare with.
    case post(text: String, clientId: Int)
    /// The sender is typing.
    case typing
}
