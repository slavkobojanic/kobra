/// Signs users in and out. Server-only, injectable only into server-side
/// services (`docs/design/26-auth-and-sessions.md`).
@ServerOnly
@Native
public service SessionManager {
    /// Starts a new session for the user, replacing any current one.
    ///
    /// - Parameters:
    ///   - userId: the user who signed in.
    ///   - roles: the user's roles; the parameter exists only when the app
    ///     declares an [`AuthRole`] enum.
    func signIn(userId: SessionUserId, roles: Set<SessionRole> = Set([])) async
    /// Ends the current session; does nothing for a guest.
    func signOut() async
    /// Ends every session of a user, on every device.
    ///
    /// - Parameter userId: the user whose sessions end.
    func endAll(userId: SessionUserId) async
    /// Issues a random value bound to the current session (creating a guest
    /// session if needed), for an OAuth `state` parameter.
    ///
    /// - Returns: the value to send; check it on return with
    ///   [`consumeState(_:)`].
    func issueState() async -> String
    /// Checks and consumes a value issued by [`issueState()`].
    ///
    /// - Parameter state: the value the OAuth provider sent back.
    /// - Returns: `true` the first time it is called with a value issued to
    ///   this session, `false` afterwards and for any other value.
    func consumeState(_ state: String) async -> Bool
}
