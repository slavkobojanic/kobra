import k.device.PushSubscriptions

/// This browser's "Notify me" for the room it shows
/// (`docs/design/50-push.md`): a push subscription, stored on the server for
/// the signed-in person and the room, so `RoomAlertJob` can tell them about
/// new messages while no tab of Huddle is open.
@Scope(.route)
public service RoomAlerts {
    @Inject let push: PushSubscriptions
    @Inject let alerts: AlertsProtocol

    /// Whether this browser can be notified here at all. It stays offered
    /// after a refusal, to say how to undo it.
    @Derived let isAvailable: Bool = push.isSupported

    /// Whether the room's new messages are pushed to this browser.
    @State var isOn: Bool = false

    /// What went wrong with the last switch, or `nil`.
    @State var problem: String? = nil

    /// Whether the server pushes at all (it has a `K_PUSH_PRIVATE_KEY`).
    @State var serverPushes: Bool = false

    /// Reads whether `room` is followed by this browser already.
    func load(room: String) async {
        isOn = false
        problem = nil

        do {
            serverPushes = try await alerts.serverKey() != nil

            if let subscription = await push.current() {
                isOn = try await alerts.isFollowing(room: room, subscription: subscription)
            }
        } catch _ {
            serverPushes = false
        }
    }

    /// Turns the room's notifications on: asks the first time (this is
    /// called from the switch's click), then subscribes with the server's
    /// key and stores the subscription for the room.
    func turnOn(room: String) async {
        problem = nil

        if push.permission == .prompt {
            await push.requestAccess()
        }

        do {
            guard let key = try await alerts.serverKey() else {
                problem = "This server doesn't send notifications."
                return
            }
            let subscription = try await push.subscribe(serverKey: key)
            try await alerts.follow(room: room, subscription: subscription)
            isOn = true
        } catch .notAsked {
            problem = "Allow notifications to be told about new messages."
        } catch .denied {
            problem = "Notifications are blocked for Huddle in this browser's site settings."
        } catch .unsupported, .unavailable {
            problem = "This browser can't receive notifications right now."
        } catch _ {
            problem = "Couldn't turn notifications on. Try again."
        }
    }

    /// Turns the room's notifications off for this browser. The
    /// subscription stays, for the other rooms it follows.
    func turnOff(room: String) async {
        problem = nil

        do {
            if let subscription = await push.current() {
                try await alerts.unfollow(room: room, subscription: subscription)
            }
            isOn = false
        } catch _ {
            problem = "Couldn't turn notifications off. Try again."
        }
    }
}
