import k.rpc.RpcFailure

/// Why a notes call that can't otherwise fail was refused.
///
/// Every `NotesProtocol` rpc requires a signed-in session, so even creating
/// a note can be refused when the session has expired.
public enum AccessError implements Error {
    /// The call failed before `NotesApi` saw it: the `Authenticated` guard
    /// refused it because nobody is signed in, or a draft failed validation
    /// on the server.
    case rpcFailure(RpcFailure)
}
