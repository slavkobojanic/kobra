import k.auth.AuthUserId
import k.crypto.Uuid

/// A customer's id, the app's user id type: stored as its value's column.
public struct CustomerId implements AuthUserId {
    /// The value.
    let value: Uuid
}
