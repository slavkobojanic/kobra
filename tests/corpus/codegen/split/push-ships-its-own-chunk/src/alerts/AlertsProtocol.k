import k.crypto.EcdsaP256PublicKey
import k.device.PushSubscription
import k.rpc.RpcOnlyError

public protocol AlertsProtocol {
    rpc serverKey() throws RpcOnlyError -> EcdsaP256PublicKey?
    rpc follow(subscription: PushSubscription) throws RpcOnlyError
}
