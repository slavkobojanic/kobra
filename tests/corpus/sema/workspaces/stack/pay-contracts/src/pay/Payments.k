import k.rpc.ServiceAuthenticated

/// Takes payments for the other services of the system.
public protocol Payments requires ServiceAuthenticated {
    /// Charges a card.
    rpc charge(request: ChargeRequest) throws ChargeError -> Charge
}
