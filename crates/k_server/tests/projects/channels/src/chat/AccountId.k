import k.auth.AuthUserId

/// The signed-in user.
public struct AccountId implements AuthUserId {
    let name: String
}
