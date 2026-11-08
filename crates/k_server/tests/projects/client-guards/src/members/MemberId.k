import k.auth.AuthUserId

/// The id of the signed-in member.
public struct MemberId implements AuthUserId {
    let name: String
}
