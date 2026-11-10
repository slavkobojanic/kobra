import k.crypto.Uuid
import k.rpc.PresenceChange

/// What the server says on a room's channel.
public enum ChatEvent implements Codable, Equatable {
    /// Every message of the room, oldest first, and who is in it: always
    /// the first event, and again after a restart.
    case snapshot(messages: List<ChatMessage>, present: List<Member>)
    /// A new message, with the client id its sender gave it.
    case posted(message: ChatMessage, clientId: Uuid)
    /// A message was deleted.
    case deleted(id: Uuid)
    /// Someone came into the room or left it.
    case presence(PresenceChange<Member>)
    /// Someone is typing; shown for a few seconds after the last one.
    case typing(name: String)
    /// The room's moderators, by name: right after the snapshot of a room
    /// that has any, and again whenever they change.
    case moderators(names: List<String>)
    /// A moderator removed `name` from the room. Their channel hears this
    /// last: the server closes it after (`ChannelClose.byServer`).
    case removed(name: String)
}
