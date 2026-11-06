/// Why a guard refused an rpc call, carried by `RpcFailure.rejected`.
public enum GuardRejection implements Error, Codable {
    /// Nobody is signed in.
    case unauthenticated
    /// The user lacks a required role.
    case forbidden
    /// The client exceeded a rate limit.
    case rateLimited(retryAfter: Duration)
}
