test CounterView {
    @Inject let counter: Counter

    test "clicking Add shows the new count" {
        let screen = render(CounterView())

        screen.button(named: "Add").click()

        expect screen.text("Count: 1").isVisible
        expect counter.value == 1
    }

    test "the limit is explained" {
        let screen = render(CounterView())

        screen.button(named: "Add").click()
        screen.button(named: "Add").click()
        screen.button(named: "Add").click()
        screen.button(named: "Add").click()

        expect screen.alert().text == "That's the limit."
        expect screen.heading(named: "Counter").level == 2
    }

    test "a missing element fails the test with the query" {
        let screen = render(CounterView())

        screen.button(named: "Remove").click()
    }
}
