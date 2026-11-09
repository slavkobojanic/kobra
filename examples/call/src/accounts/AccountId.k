import k.auth.AuthUserId

/// Who is signed in: the name they chose.
///
/// It is the app's one `AuthUserId`, so `session.user.id` has this type
/// and every message's author is typed.
public struct AccountId implements AuthUserId {
    /// The display name, as typed when signing in.
    let name: String
}
