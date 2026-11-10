import k.auth.AuthUserId

/// The signed-in person, by the name they signed up with.
public struct AccountId implements AuthUserId {
    let name: String
}
