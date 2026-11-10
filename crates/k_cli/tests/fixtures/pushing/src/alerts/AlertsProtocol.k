import k.crypto.EcdsaP256PublicKey
import k.device.PushSubscription
import k.rpc.RpcOnlyError

/// The server's push key, and a push sent at once, for anyone.
public protocol AlertsProtocol {
    /// The public half of `K_PUSH_PRIVATE_KEY`, or `nil` without one.
    rpc serverKey() throws RpcOnlyError -> EcdsaP256PublicKey?

    /// Pushes `text` to `subscription` now, and says what happened.
    rpc ping(subscription: PushSubscription, text: String) throws RpcOnlyError -> String
}
