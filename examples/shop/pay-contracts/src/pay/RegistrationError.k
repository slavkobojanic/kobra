import k.rpc.RpcFailure

/// Why a merchant couldn't register.
public enum RegistrationError implements Error, Codable {
    /// The call didn't come with a verified service token, so paybox
    /// doesn't know whose callback URL it is.
    case unknownCaller

    /// The call failed outside the handler: paybox refused the service
    /// token, was unreachable or didn't answer in time.
    case rpcFailure(RpcFailure)
}
