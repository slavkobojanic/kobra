import k.test.TestClock
import k.test.TestPageVisibility

test Refresher {
    @Inject let visibility: TestPageVisibility
    @Inject let clock: TestClock

    test "it refreshes while the page is visible and pauses while hidden" {
        let screen = render(Refresher())
        await screen.settle()

        clock.advance(by: 2.minutes)
        await screen.settle()

        expect screen.text("Refreshed 2 times").isVisible

        visibility.hide()
        clock.advance(by: 5.minutes)
        await screen.settle()

        expect screen.text("Refreshed 2 times").isVisible
        expect screen.text("Paused").isVisible

        visibility.show()
        clock.advance(by: 1.minute)
        await screen.settle()

        expect screen.text("Refreshed 3 times").isVisible
        expect not screen.text("Paused").exists
    }
}
