/// A change of who is present under a key, from [`Presence.watch`]
/// (`docs/design/44-channels.md`, "Presence"). Crosses the wire, usually as
/// a case of a channel's server message.
public enum PresenceChange<Member> implements Codable, Equatable where Member implements Codable, Member implements Equatable {
    /// The member's first lease arrived.
    case joined(Member)
    /// The member's last lease ended.
    case left(Member)
    /// A lease replaced its member with this one.
    case updated(Member)
}
