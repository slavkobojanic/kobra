// An `if let` binding in a view follows its value while the branch shows.
test Badge {
    test "a new non-nil value reaches the branch" {
        let screen = render(Badge())

        expect screen.text("Hello, Ada").isVisible

        screen.button(named: "Rename").click()

        expect screen.text("Hello, Grace").isVisible

        screen.button(named: "Clear").click()

        expect screen.text("Nobody").isVisible
    }
}
