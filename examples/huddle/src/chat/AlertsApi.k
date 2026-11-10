import k.auth.Session
import k.crypto.EcdsaP256PublicKey
import k.crypto.Uuid
import k.device.PushSubscription
import k.rpc.RpcOnlyError
import k.server.WebPush

/// The server side of `AlertsProtocol`.
public service AlertsApi implements AlertsProtocol {
    @Inject let alerts: RoomAlertRepository
    @Inject let push: WebPush
    @Inject let session: Session

    rpc serverKey() async throws RpcOnlyError -> EcdsaP256PublicKey? {
        return push.serverKey
    }

    rpc follow(room: String, subscription: PushSubscription) async throws RoomError {
        guard Rooms.names.contains(room) else {
            throw RoomError.noSuchRoom
        }

        let endpoint = subscription.endpoint

        // The browser may have been someone else's before: the alert is
        // the browser's, for whoever is signed in on it now.
        if let old = await alerts.find(room: room, endpoint: endpoint) {
            await alerts.delete(old)
        }

        await alerts.insert(RoomAlert(id: Uuid.random(), room: room, member: session.user.id.name, endpoint: endpoint, p256dh: subscription.p256dh, auth: subscription.auth))
    }

    rpc unfollow(room: String, subscription: PushSubscription) async throws RpcOnlyError {
        if let old = await alerts.find(room: room, endpoint: subscription.endpoint) {
            await alerts.delete(old)
        }
    }

    rpc isFollowing(room: String, subscription: PushSubscription) async throws RpcOnlyError -> Bool {
        return await alerts.find(room: room, endpoint: subscription.endpoint) != nil
    }
}
