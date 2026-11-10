/// A room as the lobby shows it, for the signed-in person.
public struct RoomSummary implements Codable, Equatable, Copyable {
    let name: String
    /// Joining it needs its password, once.
    let locked: Bool
    /// The number of the newest message they have seen, 0 for none.
    let lastRead: Int
    /// Messages by others after `lastRead`, or `nil` for a room they
    /// haven't joined.
    let unread: Int?
}
