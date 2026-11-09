import k.device.SystemNotification
import k.test.TestNotifications

test ExportButton {
    @Inject let notifications: TestNotifications

    test "the first click asks, then shows the notification" {
        let screen = render(ExportButton())

        screen.button(named: "Notify me when it's ready").click()
        await screen.settle()

        expect notifications.prompts == 1
        expect notifications.permission == .granted
        expect notifications.shown.count == 1
        expect screen.text("shown").isVisible
    }

    test "a refused prompt shows nothing, and the button goes" {
        notifications.answerPrompts(with: .denied)
        let screen = render(ExportButton())

        screen.button(named: "Notify me when it's ready").click()
        await screen.settle()

        expect notifications.shown.isEmpty
        expect screen.text("No notifications").isVisible
        expect not screen.button(named: "Notify me when it's ready").exists
    }

    test "clicking the notification opens its page" {
        let screen = render(ExportButton())

        screen.button(named: "Notify me when it's ready").click()
        await screen.settle()
        notifications.click(notifications.shown[0])
        await screen.settle()

        expect screen.currentPage == .report(id: "42")
    }

    test "clicking a notification without a page leaves the tab where it is" {
        let screen = render(ExportButton())
        notifications.setPermission(.granted)
        let done = SystemNotification(title: "Done")
        try await notifications.show(done)
        Router.navigate(to: .report(id: "1"))

        notifications.click(done)
        await screen.settle()

        expect screen.currentPage == .report(id: "1")
    }
}
