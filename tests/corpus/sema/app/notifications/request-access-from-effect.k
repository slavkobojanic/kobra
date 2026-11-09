// Only `Notifications.requestAccess()` prompts, and it is `@UserAction` (45-web-platform.md,
// "Notifications"): asking from an effect is K4030, through the service that asks too. Showing a
// notification never prompts, so a timer's callback or an effect may show one once allowed.
import k.device.NotificationError
import k.device.Notifications
import k.device.SystemNotification

service Alerts {
    @Inject let notifications: Notifications

    @Derived let canNotify: Bool = notifications.isSupported and notifications.permission == .granted

    func ask() async -> Bool {
        return await notifications.requestAccess() == .granted
    }

    func tell(_ title: String) async throws NotificationError {
        try await notifications.show(SystemNotification(title: title))
    }
}

component Countdown {
    @Inject let alerts: Alerts

    @Effect
    func askAtOnce() async {
        let granted = await alerts.ask()
    }

    @Effect
    func tellWhenDone() async {
        do {
            try await alerts.tell("Time's up")
        } catch _ {
        }
    }

    view {
        if alerts.canNotify {
            text { "on" }
        }
        button { "Notify me" }
            .on(click: { await alerts.ask() })
    }
}
