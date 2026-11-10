import k.crypto.Uuid

/// What a browser says on a room's channel.
public enum ChatCommand implements Codable, Equatable {
    /// Posts `text`. `clientId` comes back with the echo, so the sender
    /// replaces its pending copy with the stored message.
    case post(text: String, clientId: Uuid)
    /// Deletes one of the sender's own messages, or, from a moderator,
    /// anyone's.
    case delete(id: Uuid)
    /// The sender is typing. Nothing stores it: it is passed on to the room.
    case typing
    /// The sender has seen the room's messages up to number `upTo`.
    case read(upTo: Int)
    /// A moderator makes the member `name` a moderator too.
    case promote(name: String)
    /// A moderator removes the member `name` from the room for good.
    case remove(name: String)
}
