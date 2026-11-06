import k.auth.GuardRejection
import k.validation.ValidationError

/// A failure outside an rpc's handler, carried by the `rpcFailure` case
/// every rpc error enum has (`docs/design/28-rpc-wire-format.md`, "Failures
/// outside the handler"). Only the runtime produces it.
public enum RpcFailure implements Error, Codable {
    /// A guard refused the call.
    case rejected(GuardRejection)
    /// A `Validatable` parameter failed validation.
    case invalid(ValidationError)
    /// The browser couldn't reach the server.
    case offline
    /// The server didn't answer in time.
    case timeout
}
