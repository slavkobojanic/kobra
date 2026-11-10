import k.rpc.RpcFailure

/// Why creating an account failed.
public enum SignUpError implements Error {
    /// Another account registered the same email between validation and
    /// saving. The `@Check` on `SignUpRequest.email` catches the common case
    /// earlier, under the email field.
    case emailTaken

    /// The server refused the new passkey: its challenge was used or
    /// expired, it was made on another site, or the device didn't verify
    /// the person.
    case passkeyNotAccepted

    /// The call failed outside `AuthApi`: the rate limit refused it, the
    /// request failed validation on the server, or the browser couldn't
    /// reach the server.
    case rpcFailure(RpcFailure)
}
