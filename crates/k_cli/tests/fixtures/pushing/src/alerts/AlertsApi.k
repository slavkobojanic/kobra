import k.crypto.EcdsaP256PublicKey
import k.device.PushSubscription
import k.device.SystemNotification
import k.rpc.RpcOnlyError
import k.server.WebPush

/// The server side of `AlertsProtocol`.
public service AlertsApi implements AlertsProtocol {
    @Inject let push: WebPush

    rpc serverKey() async throws RpcOnlyError -> EcdsaP256PublicKey? {
        return push.serverKey
    }

    rpc ping(subscription: PushSubscription, text: String) async throws RpcOnlyError -> String {
        do {
            try await push.send(SystemNotification(title: "Ping", body: text, tag: "ping", opens: .report(id: "7")), to: subscription, timeToLive: 1.minutes)
            return "sent"
        } catch .expired {
            return "expired"
        } catch .unavailable {
            return "unavailable"
        } catch .rejected(let status) {
            return "rejected {status}"
        } catch .tooLarge {
            return "too large"
        } catch .invalidSubscription {
            return "invalid subscription"
        } catch .notConfigured {
            return "not configured"
        }
    }
}
