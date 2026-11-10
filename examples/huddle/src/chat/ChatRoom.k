import k.crypto.Uuid

/// A room, stored in the `chat_room` table. `general`, `random` and `help`
/// come with the app (migration 0003); people create the others from the
/// lobby. Server-only; the lobby sees a `RoomSummary`.
public entity ChatRoom {
    @Id let id: Uuid

    /// Its name, as in `/room/{name}`.
    @Unique let name: String

    /// Who created it, its first moderator; `nil` for the rooms that come
    /// with the app, which have no moderators.
    let createdBy: String?

    /// The Argon2id hash of its password (`k.crypto.Password`), or `nil`
    /// for a room anyone may join.
    let passwordHash: String?

    /// The number of its newest message. Each post adds one in the
    /// transaction that stores it, so numbers are never reused, even after
    /// a deletion.
    var lastNumber: Int
}
