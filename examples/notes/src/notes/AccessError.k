import k.auth.GuardRejection

/// Why a notes call that can't otherwise fail was refused.
///
/// Every `NotesProtocol` rpc requires a signed-in session, so even creating
/// a note can be refused when the session has expired.
public enum AccessError implements Error {
    /// The `Authenticated` guard refused the call: nobody is signed in.
    case rejected(GuardRejection)
}
