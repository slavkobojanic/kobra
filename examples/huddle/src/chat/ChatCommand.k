import k.crypto.Uuid

/// What a browser says on a room's channel.
public enum ChatCommand implements Codable, Equatable {
    /// Posts `text`. `clientId` comes back with the echo, so the sender
    /// replaces its pending copy with the stored message.
    case post(text: String, clientId: Uuid)
    /// Deletes one of the sender's own messages.
    case delete(id: Uuid)
}
