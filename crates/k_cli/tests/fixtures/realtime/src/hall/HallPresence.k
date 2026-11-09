import k.rpc.Presence
import k.rpc.PresenceChange
import k.rpc.PresenceLease

/// Who is in each hall, one instance per server process.
@Scope(.server)
public service HallPresence {
    private let presence: Presence<String, Member> = Presence()

    /// Adds `member` to `hall` until the calling request ends.
    func join(hall: String, member: Member) -> PresenceLease<Member> {
        return presence.join(key: hall, member: member)
    }

    /// Everyone in `hall`, in the order they arrived.
    func members(hall: String) async -> List<Member> {
        return await presence.members(key: hall)
    }

    /// The changes in `hall` from now on.
    func watch(hall: String) -> Stream<PresenceChange<Member>> {
        return presence.watch(key: hall)
    }
}
