import k.device.SystemNotification
import k.encoding.Json
import k.test.TestNotifications

test ExportService {
    @Inject let notifications: TestNotifications
    @Inject let exports: ExportService

    test "the double starts capable, at prompt, with nothing shown" {
        expect notifications.isSupported
        expect notifications.permission == .prompt
        expect notifications.prompts == 0
        expect notifications.shown.isEmpty
        expect exports.canNotify
    }

    test "showing before asking is not asked, and never prompts" {
        expect await exports.tryAnnounce(id: "1") == "not asked"
        expect notifications.prompts == 0
        expect notifications.shown.isEmpty
    }

    test "a granted permission shows the notification as given" {
        notifications.setPermission(.granted)

        expect await exports.tryAnnounce(id: "7") == "shown"
        expect notifications.shown == [SystemNotification(title: "Your export is ready", body: "Report 7", tag: "report-7", opens: .report(id: "7"))]
    }

    test "every notification is recorded, a replaced one too" {
        notifications.setPermission(.granted)

        expect await exports.tryAnnounce(id: "7") == "shown"
        expect await exports.tryAnnounce(id: "7") == "shown"
        expect notifications.shown.count == 2
    }

    test "a denied permission is denied, and hides the offer" {
        notifications.setPermission(.denied)

        expect await exports.tryAnnounce(id: "1") == "denied"
        expect not exports.canNotify
        expect notifications.shown.isEmpty
    }

    test "without notifications every show is unsupported, and nothing asks" {
        notifications.setSupported(false)

        expect not notifications.isSupported
        expect not exports.canNotify
        expect await exports.tryAnnounce(id: "1") == "unsupported"
        expect await notifications.requestAccess() == .prompt
        expect notifications.prompts == 0
    }

    test "requestAccess asks once, and answers the permission after" {
        expect await notifications.requestAccess() == .granted
        expect await notifications.requestAccess() == .granted
        expect notifications.prompts == 1
    }

    test "a prompt dismissed without an answer stays at prompt, and asks again" {
        notifications.answerPrompts(with: .prompt)

        expect await notifications.requestAccess() == .prompt
        expect await notifications.requestAccess() == .prompt
        expect notifications.prompts == 2
    }

    test "a notification's fields default to nil" {
        let plain = SystemNotification(title: "Done")

        expect plain.body == nil
        expect plain.tag == nil
        expect plain.opens == nil
        expect plain != SystemNotification(title: "Done", tag: "t")
    }

    test "a notification survives JSON, its page too" {
        let n = SystemNotification(title: "Ready", body: "Report 3", tag: "report-3", opens: .report(id: "3"))

        let back: SystemNotification = try Json.decode(Json.encode(n))

        expect back == n
    }
}
