/// Who is signed in, readable on both sides
/// (`docs/design/26-auth-and-sessions.md`). In the browser it is a read-only
/// mirror of the server's session.
@Scope(.app)
@Native
public service Session {
    /// The signed-in user, or `nil` for a guest. Reactive. Its type is
    /// `SessionUser`, not optional, inside bodies guarded by `Authenticated`.
    var user: SessionUser?
}
