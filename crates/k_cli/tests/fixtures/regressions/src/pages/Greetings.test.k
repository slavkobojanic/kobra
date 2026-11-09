// Two test groups in one file. The second is named after `Wave`, whose
// file sorts after this one: the group used to take the name `Wave`
// itself, so `Wave()` constructed the group ("Class constructor $groupN
// cannot be invoked without 'new'").
test Greetings {
    test "the badge greets Ada" {
        let screen = render(Badge())

        expect screen.text("Hello, Ada").isVisible
    }
}

test Wave {
    test "a click waves" {
        let screen = render(Wave())

        screen.button(named: "Wave").click()

        expect screen.text("Waved").isVisible
    }
}
