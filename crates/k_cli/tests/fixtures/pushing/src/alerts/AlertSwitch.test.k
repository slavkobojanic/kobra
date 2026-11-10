import k.test.TestPushSubscriptions

test AlertSwitch {
    @Inject let push: TestPushSubscriptions

    test "the click asks and subscribes" {
        let screen = render(AlertSwitch())

        screen.button(named: "Notify me").click()
        await screen.settle()

        expect push.prompts == 1
        expect push.subscription != nil
        expect screen.text("on").isVisible
    }

    test "a refusal says so, and the button goes" {
        push.answerPrompts(with: .denied)
        let screen = render(AlertSwitch())

        screen.button(named: "Notify me").click()
        await screen.settle()

        expect screen.text("denied").isVisible
        expect not screen.button(named: "Notify me").exists
    }
}
