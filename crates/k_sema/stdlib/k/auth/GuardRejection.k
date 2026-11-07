/// Why a guard refused an rpc call, carried by [`k.rpc.RpcFailure.rejected`].
public enum GuardRejection implements Error, Codable {
    /// Nobody is signed in ([`Authenticated`] refused the call).
    case unauthenticated
    /// The user lacks a required role ([`Role`] refused the call).
    case forbidden
    /// The client exceeded a [`RateLimit`]; it may call again after
    /// `retryAfter`.
    case rateLimited(retryAfter: Duration)
}
