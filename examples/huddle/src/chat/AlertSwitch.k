/// The "Notify me" switch of a room: shown where the browser can receive
/// push messages and the server sends them. Rendered in the browser only,
/// since the server can't know this browser's subscription.
@Rendering(.client)
component AlertSwitch {
    /// The room it is for.
    @Prop let room: String

    @Inject let alerts: RoomAlerts

    @Effect(on: [room])
    func load() async {
        await alerts.load(room: room)
    }

    loading {
    }

    view {
        if alerts.isAvailable and alerts.serverPushes {
            if alerts.isOn {
                button { "Stop notifying me" }
                    .on(click: { await alerts.turnOff(room: room) })
            } else {
                button { "Notify me of new messages" }
                    .on(click: { await alerts.turnOn(room: room) })
            }
        }

        if let problem = alerts.problem {
            paragraph { problem }
                .role(.status)
        }
    }
}
