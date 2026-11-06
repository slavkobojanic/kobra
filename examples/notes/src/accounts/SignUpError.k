import k.rpc.RpcFailure

/// Why creating an account failed.
public enum SignUpError implements Error {
    /// Another account registered the same email between validation and
    /// saving. The `@Check` on `SignUpRequest.email` catches the common case
    /// earlier, under the email field.
    case emailTaken

    /// The call failed before `AuthApi` saw it: the rate limit refused it,
    /// or the request failed validation on the server.
    case rpcFailure(RpcFailure)
}
