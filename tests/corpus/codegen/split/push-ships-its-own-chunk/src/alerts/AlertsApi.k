import k.crypto.EcdsaP256PublicKey
import k.device.PushSubscription
import k.device.SystemNotification
import k.rpc.RpcOnlyError
import k.server.WebPush

public service AlertsApi implements AlertsProtocol {
    @Inject let push: WebPush

    rpc serverKey() async throws RpcOnlyError -> EcdsaP256PublicKey? {
        return push.serverKey
    }

    rpc follow(subscription: PushSubscription) async throws RpcOnlyError {
        do {
            try await push.send(SystemNotification(title: "Following", opens: .home), to: subscription)
        } catch _ {
        }
    }
}
