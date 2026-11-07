import k.auth.AuthUserId

/// The app's user id type, required by `Session`.
public struct AuthUser implements AuthUserId {
    let name: String
}
