import accounts.AccountId
import k.auth.SessionManager
import k.test.RecordingJobQueue
import k.test.RecordingWebPush
import k.test.TestPushSubscriptions

test AlertSwitch {
    @Inject let sessions: SessionManager
    @Inject let push: TestPushSubscriptions
    @Inject let server: RecordingWebPush
    @Inject let alerts: RoomAlertRepository
    @Inject let jobs: RecordingJobQueue

    test "notify me asks once, subscribes and follows the room" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(name: "general"))
        await screen.settle()

        screen.button(named: "Notify me of new messages").click()
        await screen.settle()

        expect push.prompts == 1
        expect push.serverKey == server.serverKey
        let stored = await alerts.of(room: "general")
        expect stored.map { a => a.member } == ["ana"]
        expect stored.map { a => a.endpoint } == [push.subscription?.endpoint ?? Url("https://nothing.example")]
        expect screen.button(named: "Stop notifying me").exists
    }

    test "stop notifying me unfollows the room, and a reload remembers" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(name: "general"))
        await screen.settle()
        screen.button(named: "Notify me of new messages").click()
        await screen.settle()

        let again = render(Room(name: "general"))
        await again.settle()
        expect again.button(named: "Stop notifying me").exists

        again.button(named: "Stop notifying me").click()
        await again.settle()

        expect (await alerts.of(room: "general")).isEmpty
        expect again.button(named: "Notify me of new messages").exists
    }

    test "a refusal says how to undo it" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        push.answerPrompts(with: .denied)
        let screen = render(Room(name: "general"))
        await screen.settle()

        screen.button(named: "Notify me of new messages").click()
        await screen.settle()

        expect screen.text("Notifications are blocked for Huddle in this browser's site settings.").isVisible
        expect (await alerts.of(room: "general")).isEmpty
    }

    test "without a server key there is no switch" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        server.setConfigured(false)
        let screen = render(Room(name: "general"))
        await screen.settle()

        expect not screen.button(named: "Notify me of new messages").exists
    }

    test "a posted message is handed to the alert job" {
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(name: "general"))
        await screen.settle()

        screen.textField(labeled: "Message").fill("Lunch?")
        screen.button(named: "Send").click()
        await screen.settle()

        expect jobs.payloads(of: RoomAlertJob.kind) == [NewMessageAlert(room: "general", author: "ana", text: "Lunch?")]
    }
}
