import k.auth.AuthUserId

/// The id of the signed-in user.
public struct AccountId implements AuthUserId {
    let name: String
}
