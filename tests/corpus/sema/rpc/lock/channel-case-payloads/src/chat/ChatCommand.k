/// What a client says: its cases' fields follow a struct's rules.
public enum ChatCommand implements Codable {
    /// Posts a message: a new required field breaks open tabs, which don't send it.
    case post(text: String, clientId: Int)
    /// Edits a message: a changed field type breaks.
    case edit(id: String, text: String)
    /// The sender is typing: a new optional field is compatible.
    case typing(draft: String?)
    /// A new case may have required fields.
    case react(id: Int, emoji: String)
}
