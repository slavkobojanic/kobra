import k.auth.AuthUserId
import k.crypto.Uuid

/// The id of an account, and of the person signed in.
public struct AccountId implements AuthUserId {
    /// The random id behind it.
    let value: Uuid
}
