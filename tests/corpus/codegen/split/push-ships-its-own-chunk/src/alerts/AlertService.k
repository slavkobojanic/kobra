import k.device.PushSubscriptions

public service AlertService {
    @Inject let push: PushSubscriptions
    @Inject let alerts: AlertsProtocol

    func subscribe() async {
        await push.requestAccess()
        do {
            if let key = try await alerts.serverKey() {
                try await alerts.follow(subscription: try await push.subscribe(serverKey: key))
            }
        } catch _ {
        }
    }
}
