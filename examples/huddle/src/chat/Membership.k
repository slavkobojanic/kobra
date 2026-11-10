import k.crypto.Uuid

/// Someone who joined a room, stored in the `membership` table: where they
/// stopped reading, whether they moderate it, and whether a moderator
/// removed them. Joining a room with a password stores one too, so the
/// password is asked for once per person.
public entity Membership {
    @Id let id: Uuid

    /// `room` and `member` together (`"{room} {member}"`; neither has a
    /// space), so two tabs joining at once store one membership.
    @Unique let key: String

    @Index let room: String
    let member: String

    /// The number of the newest message they have seen.
    var lastRead: Int

    /// Moderators delete anyone's messages, make others moderators and
    /// remove members. A room's creator is its first.
    var moderator: Bool

    /// `true` once a moderator removed them: they can't come back.
    var removed: Bool
}
