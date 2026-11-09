import k.test.TestClipboard
import k.test.TestClock

test InvitePanel {
    @Inject let clipboard: TestClipboard
    @Inject let clock: TestClock

    test "copying the invite link puts it on the clipboard and says so" {
        let screen = render(InvitePanel(invite: Invite.sample))

        screen.button(named: "Copy invite link").click()
        await screen.settle()

        expect clipboard.text == "https://example.com/join/42"
        expect screen.status().text == "Copied"
        expect screen.button(named: "Copy invite link").text == "Copied"
    }

    test "the button says Copied for two seconds, then shows its label again" {
        let screen = render(InvitePanel(invite: Invite.sample))

        screen.button(named: "Copy invite link").click()
        await screen.settle()
        clock.advance(by: 1900.milliseconds)

        expect screen.button(named: "Copy invite link").text == "Copied"

        clock.advance(by: 200.milliseconds)

        expect screen.button(named: "Copy invite link").text == "Copy invite link"
        expect screen.status().text == ""
    }

    test "a failure nobody handles shows Couldn't copy" {
        clipboard.failWrites(error: .denied)
        let screen = render(InvitePanel(invite: Invite.sample))

        screen.button(named: "Copy invite link").click()
        await screen.settle()

        expect clipboard.text == nil
        expect screen.status().text == "Couldn't copy"
    }

    test "the clipboard starts empty and records each call" {
        expect clipboard.text == nil
        expect clipboard.calls == 0
        expect clipboard.isSupported

        let screen = render(InvitePanel(invite: Invite.sample))
        screen.button(named: "Copy invite link").click()
        await screen.settle()

        expect clipboard.calls == 1
    }
}
