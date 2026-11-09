// `show` throws `NotificationError`, whose three cases a `catch` must cover (45-web-platform.md,
// "Notifications"); a `SystemNotification` needs only its title, and opens a typed page.
import k.device.Notifications
import k.device.SystemNotification

page Inbox at "/inbox" {
    view {
        text { "inbox" }
    }
}

service Alerts {
    @Inject let notifications: Notifications

    func say(_ title: String) async -> String {
        do {
            try await notifications.show(SystemNotification(title: title, opens: .inbox))
            return "shown"
        } catch .unsupported {
            return "unsupported"
        } catch .notAsked {
            return "not asked"
        }
    }

    func sayPlainly() async {
        try await notifications.show(SystemNotification(title: "Done", body: "All done", tag: "done"))
    }
}
