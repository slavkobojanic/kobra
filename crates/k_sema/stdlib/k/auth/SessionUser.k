/// The signed-in user, generated for the app
/// (`docs/design/26-auth-and-sessions.md`, "SessionUser is generated").
/// Needs no import.
@Generated
public struct SessionUser implements Equatable, Codable {
    /// The user's id: the app's one type implementing [`AuthUserId`].
    let id: SessionUserId
    /// The user's roles; exists only when the app declares an
    /// [`AuthRole`] enum.
    let roles: Set<SessionRole>
}
