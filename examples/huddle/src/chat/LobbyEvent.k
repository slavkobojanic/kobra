/// What the server says on the lobby's channel: enough for the browser to
/// keep each room's unread count without asking again.
public enum LobbyEvent implements Codable, Equatable {
    /// Every room, with the person's place and count in each: always the
    /// first event, and again after a restart.
    case snapshot(rooms: List<RoomSummary>)
    /// Someone created a room.
    case created(room: RoomSummary)
    /// Message `number` of `room` was posted by `author`.
    case posted(room: String, number: Int, author: String)
    /// Message `number` of `room`, by `author`, was deleted.
    case deleted(room: String, number: Int, author: String)
    /// The person read `room` up to message `upTo`, in any of their tabs,
    /// and has `unread` messages left there.
    case read(room: String, upTo: Int, unread: Int)
}
