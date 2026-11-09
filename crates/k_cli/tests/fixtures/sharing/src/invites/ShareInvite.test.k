import k.device.ShareContent
import k.test.TestClipboard
import k.test.TestShare

test ShareInvite {
    @Inject let share: TestShare
    @Inject let clipboard: TestClipboard

    test "where the browser can share, the button opens the sheet" {
        let screen = render(ShareInvite(invite: Invite.sample))

        screen.button(named: "Share invite").click()
        await screen.settle()

        expect share.shared == [ShareContent.link(Url("https://example.com/join/42"), title: "Join Ops")]
        expect screen.text("Outcome: shared").isVisible
        expect clipboard.text == nil
    }

    test "closing the sheet is no failure and fires nothing" {
        share.cancelNext()
        let screen = render(ShareInvite(invite: Invite.sample))

        screen.button(named: "Share invite").click()
        await screen.settle()

        expect share.shared.isEmpty
        expect screen.text("Outcome: nothing yet").isVisible
    }

    test "a refused share fires failed" {
        share.failNext(error: .denied)
        let screen = render(ShareInvite(invite: Invite.sample))

        screen.button(named: "Share invite").click()
        await screen.settle()

        expect screen.text("Outcome: refused").isVisible
    }

    test "where the browser can't share, the button copies the link and says so" {
        share.setSupported(false)
        let screen = render(ShareInvite(invite: Invite.sample))

        screen.button(named: "Share invite").click()
        await screen.settle()

        expect share.shared.isEmpty
        expect clipboard.text == "https://example.com/join/42"
        expect screen.status().text == "Link copied"
        expect screen.text("Outcome: copied").isVisible
    }

    test "where it can neither share nor copy, the failure is unsupported" {
        share.setSupported(false)
        clipboard.setSupported(false)
        let screen = render(ShareInvite(invite: Invite.sample))

        screen.button(named: "Share invite").click()
        await screen.settle()

        expect screen.text("Outcome: impossible").isVisible
    }
}
