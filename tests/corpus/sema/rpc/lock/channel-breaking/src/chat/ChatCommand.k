/// What a client says: a removed case breaks open tabs, which still send it.
public enum ChatCommand implements Codable {
    /// Posts a message.
    case post(text: String)
}
