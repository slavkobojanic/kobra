import k.crypto.Uuid

/// What the server says on a room's channel.
public enum ChatEvent implements Codable, Equatable {
    /// Every message of the room, oldest first: always the first event, and
    /// again after a restart.
    case snapshot(messages: List<ChatMessage>)
    /// A new message, with the client id its sender gave it.
    case posted(message: ChatMessage, clientId: Uuid)
    /// A message was deleted.
    case deleted(id: Uuid)
}
