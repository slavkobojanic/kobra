/// What a browser says on the lobby's channel.
public enum LobbyCommand implements Codable, Equatable {
    /// Creates the room `name`, with the sender as its moderator. A
    /// `password` makes joining it need that password.
    case create(name: String, password: String?)
}
