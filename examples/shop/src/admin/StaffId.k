import k.auth.AuthUserId
import k.text.EmailAddress

/// The id of a signed-in member of staff, and so the type of
/// `session.user.id`.
///
/// This is the app's one `AuthUserId` type. Customers check out as guests
/// and never sign in, and the one administrator is configured in
/// `AdminConfig` rather than stored in a table, so a member of staff is
/// identified by the email address they signed in with.
public struct StaffId implements AuthUserId {
    /// The email address the member of staff signed in with.
    let email: EmailAddress
}
