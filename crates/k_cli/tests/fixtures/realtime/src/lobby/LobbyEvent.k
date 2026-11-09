/// What the server says on the lobby's channel.
public enum LobbyEvent implements Codable {
    /// Every line so far, sent first.
    case snapshot(lines: List<String>)
    /// A new line.
    case said(line: String)
}
