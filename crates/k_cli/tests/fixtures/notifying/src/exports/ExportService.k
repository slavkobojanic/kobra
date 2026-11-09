import k.device.NotificationError
import k.device.Notifications
import k.device.SystemNotification

/// Says when an export is ready, with a notification the operating system
/// shows even while the tab is in the background.
public service ExportService {
    @Inject let notifications: Notifications

    /// Whether to offer notifications: hidden where the browser can't show
    /// them and once the user has blocked them.
    @Derived let canNotify: Bool = notifications.isSupported and notifications.permission != .denied

    /// Asks the first time (from a click), and says whether the app may
    /// notify now.
    func enable() async -> Bool {
        if notifications.permission == .prompt {
            return await notifications.requestAccess() == .granted
        }

        return notifications.permission == .granted
    }

    /// Announces that report `id` is ready; a later announcement of the
    /// same report replaces this one.
    func announce(id: String) async throws NotificationError {
        try await notifications.show(SystemNotification(title: "Your export is ready", body: "Report {id}", tag: "report-{id}", opens: .report(id: id)))
    }

    /// Announces a report, saying what went wrong instead of throwing.
    func tryAnnounce(id: String) async -> String {
        do {
            try await announce(id: id)
            return "shown"
        } catch .unsupported {
            return "unsupported"
        } catch .notAsked {
            return "not asked"
        } catch .denied {
            return "denied"
        }
    }
}
