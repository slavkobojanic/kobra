import k.auth.AuthRole

/// The roles a member can hold.
public enum MemberRole implements AuthRole {
    /// May open the admin area.
    case admin
}
