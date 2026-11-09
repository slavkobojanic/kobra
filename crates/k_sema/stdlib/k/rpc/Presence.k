/// Who is connected to a subject, such as the people in a chat room
/// (`docs/design/44-channels.md`, "Presence"), on top of [`Broadcast`].
/// Held by a `@Scope(.server)` service ([`k.core.ServiceScope.server`]).
///
/// A member joins with a [`PresenceLease`] that belongs to the request that
/// took it: for a channel, the member leaves when the channel closes, with
/// nothing to clean up. Equal members are counted, not listed, so two tabs
/// of one person are one member.
@ServerOnly
@Native
public class Presence<Key, Member> where Key implements Hashable, Key implements Codable, Member implements Hashable, Member implements Codable {
    /// Creates a presence with nobody present.
    @Native
    init() {}

    /// Adds `member` under `key` for as long as the calling request lives;
    /// for a channel, until it closes. The first lease of a member
    /// announces [`PresenceChange.joined`].
    ///
    /// - Returns: the lease, to update the member or leave early.
    func join(key: Key, member: Member) -> PresenceLease<Member>
    /// Everyone present under `key`, each member once, in the order they
    /// arrived.
    func members(key: Key) async -> List<Member>
    /// The changes under `key` from now on: `joined`, `left` and `updated`.
    func watch(key: Key) -> Stream<PresenceChange<Member>>
}
