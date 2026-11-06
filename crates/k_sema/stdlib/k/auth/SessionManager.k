/// Signs users in and out. Server-only, injectable only into server-side
/// services (`docs/design/26-auth-and-sessions.md`).
@ServerOnly
@Native
public service SessionManager {
    /// Starts a new session for the user, replacing any current one. The
    /// `roles` parameter exists only when the app declares an `AuthRole`
    /// enum.
    func signIn(userId: SessionUserId, roles: Set<SessionRole> = Set([])) async
    /// Ends the current session; does nothing for a guest.
    func signOut() async
    /// Ends every session of a user, on every device.
    func endAll(userId: SessionUserId) async
    /// A random value bound to the current session, for an OAuth `state`.
    func issueState() async -> String
    /// `true` once, if `state` was issued to this session.
    func consumeState(_ state: String) async -> Bool
}
