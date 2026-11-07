import k.auth.AuthUserId

/// The signed-in user.
public struct UserId implements AuthUserId {
    let name: String
}
