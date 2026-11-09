import k.test.TestWakeLock

test Recipe {
    @Inject let wakeLock: TestWakeLock

    test "the screen stays on only while the step-by-step mode is shown" {
        let screen = render(Recipe())

        expect not wakeLock.isHeld

        screen.button(named: "Step by step").click()

        expect wakeLock.isHeld

        screen.button(named: "Step by step").click()

        expect not wakeLock.isHeld
    }

    test "a refused lock fires failed" {
        wakeLock.refuse(with: .unavailable)
        let screen = render(Recipe())

        screen.button(named: "Step by step").click()

        expect not wakeLock.isHeld
        expect screen.text("The screen may turn off.").isVisible
    }

    test "a browser without a wake lock says so" {
        wakeLock.refuse(with: .unsupported)
        let screen = render(Recipe())

        screen.button(named: "Step by step").click()

        expect screen.text("This browser can't keep the screen on.").isVisible
    }
}
