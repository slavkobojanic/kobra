import k.auth.AuthUserId

/// The signed-in person.
public struct AccountId implements AuthUserId {
    let name: String
}
