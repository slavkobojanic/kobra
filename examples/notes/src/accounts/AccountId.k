import k.auth.AuthUserId
import k.crypto.Uuid

/// The id of an account, and of the signed-in user.
///
/// It is the app's one `AuthUserId`, so `SessionUser.id` has this type and
/// every note's owner is typed, never text built by interpolation.
public struct AccountId implements AuthUserId {
    /// The random UUID behind the id.
    let value: Uuid
}
