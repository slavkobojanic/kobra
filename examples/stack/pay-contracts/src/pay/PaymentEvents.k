import k.rpc.RpcOnlyError
import k.rpc.ServiceAuthenticated

/// What paybox tells the services it charges for. Served by shop and called
/// by paybox: the two services call each other, so both depend on this
/// library instead of on each other.
public protocol PaymentEvents requires ServiceAuthenticated {
    /// `charge` was taken.
    rpc paid(charge: Charge) throws RpcOnlyError
}
