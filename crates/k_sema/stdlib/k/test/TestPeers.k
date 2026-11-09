import k.auth.SessionRole
import k.auth.SessionUserId

/// More browsers in one test (`docs/design/44-channels.md`, "TestPeer"):
/// realtime features are about other people, so a test can sign in a
/// second person and act as them against the same server, database and
/// clock.
@Native
public service TestPeers {
    /// Opens another browser and signs `userId` in there, with its own
    /// session; the test's own browser stays signed in as it was.
    ///
    /// - Returns: the new browser.
    func signIn(userId: SessionUserId, roles: Set<SessionRole> = Set([])) async -> TestPeer
    /// Opens another browser with nobody signed in.
    func guest() -> TestPeer
}
