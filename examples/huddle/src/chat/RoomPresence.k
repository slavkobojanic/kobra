import k.rpc.Presence
import k.rpc.PresenceChange
import k.rpc.PresenceLease

/// Who is in each room, one instance per server process. With Postgres the
/// leases are shared by every replica, so a room lists the people
/// connected to any of them.
@Scope(.server)
public service RoomPresence {
    private let presence: Presence<String, Member> = Presence()

    /// Adds `member` to `room` until the channel (the request) that called
    /// this closes: nothing to clean up, even after a crash.
    func join(room: String, member: Member) -> PresenceLease<Member> {
        return presence.join(key: room, member: member)
    }

    /// Everyone in `room`, each once, in the order they arrived.
    func members(room: String) async -> List<Member> {
        return await presence.members(key: room)
    }

    /// Arrivals and departures in `room` from now on.
    func watch(room: String) -> Stream<PresenceChange<Member>> {
        return presence.watch(key: room)
    }
}
