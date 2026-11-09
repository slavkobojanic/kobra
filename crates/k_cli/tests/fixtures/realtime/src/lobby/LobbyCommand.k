/// What a person says on the lobby's channel.
public enum LobbyCommand implements Codable, Equatable {
    /// Adds a line to the lobby.
    case say(text: String)
}
