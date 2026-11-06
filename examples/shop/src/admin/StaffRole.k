import k.auth.AuthRole

/// The roles a signed-in member of staff can hold.
///
/// This is the app's one `AuthRole` enum, so `Role(.admin)` in a `requires`
/// clause resolves against it. Customers never sign in, so there is no
/// customer role.
public enum StaffRole implements AuthRole {
    /// May see every order and mark orders as shipped.
    case admin
}
