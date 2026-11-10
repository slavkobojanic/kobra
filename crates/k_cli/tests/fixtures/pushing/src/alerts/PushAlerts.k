import k.device.PushSubscription
import k.device.PushSubscriptions

/// This browser's push subscription for the app's alerts.
public service PushAlerts {
    @Inject let push: PushSubscriptions
    @Inject let alerts: AlertsProtocol

    /// Whether to offer alerts: hidden where the browser can't receive
    /// pushes and once the user has blocked notifications.
    @Derived let canOffer: Bool = push.isSupported and push.permission != .denied

    /// The subscription made here, `nil` before.
    @State var subscription: PushSubscription? = nil

    /// Asks the first time (from a click), subscribes with the server's
    /// key, and says what happened.
    func turnOn() async -> String {
        if push.permission == .prompt {
            await push.requestAccess()
        }

        do {
            guard let key = try await alerts.serverKey() else {
                return "not configured"
            }
            subscription = try await push.subscribe(serverKey: key)
            return "on"
        } catch .notAsked {
            return "not asked"
        } catch .denied {
            return "denied"
        } catch .unsupported {
            return "unsupported"
        } catch .unavailable {
            return "unavailable"
        } catch _ {
            return "the server didn't answer"
        }
    }

    /// Ends the subscription.
    func turnOff() async {
        await push.unsubscribe()
        subscription = nil
    }
}
