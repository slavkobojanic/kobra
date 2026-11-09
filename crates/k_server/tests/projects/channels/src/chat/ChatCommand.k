/// What a client says on a room's channel.
public enum ChatCommand implements Codable {
    /// Stores `text` and announces it to the room.
    case post(text: String)
    /// Always refused with `CommandError.notAllowed`.
    case refuse
    /// Announces how many messages this channel has handled, itself included.
    case count
    /// Announces who sent it.
    case whoami
}
