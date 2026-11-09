import k.device.Notifications
import k.device.SystemNotification

public service AlertService {
    @Inject let notifications: Notifications

    @Derived let canNotify: Bool = notifications.isSupported and notifications.permission != .denied

    func enable() async {
        if notifications.permission == .prompt {
            let answer = await notifications.requestAccess()
        }
        do {
            try await notifications.show(SystemNotification(title: "Ready", opens: .report(id: "1")))
        } catch _ {
        }
    }
}
