/// One member's presence under one key, from [`Presence.join`]
/// (`docs/design/44-channels.md`, "Presence"). It ends when the request
/// that took it does, or at [`leave`].
@ServerOnly
@Native
public class PresenceLease<Member> {
    /// Replaces this lease's member (a status such as "away") and announces
    /// [`PresenceChange.updated`].
    func update(member: Member)
    /// Leaves now instead of when the request ends; idempotent.
    func leave()
}
