import k.rpc.ServiceAuthenticated

/// Takes payments for the other services of the system. Served by paybox
/// and called by shop, never by a browser: `ServiceAuthenticated` admits
/// only callers holding a token signed with the shared secret.
public protocol Payments requires ServiceAuthenticated {
    /// Charges `request.cents` to a card. Throws `.declined` for an amount
    /// that isn't positive.
    rpc charge(request: ChargeRequest) throws ChargeError -> Charge
}
