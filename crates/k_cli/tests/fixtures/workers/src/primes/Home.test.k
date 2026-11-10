/// The page awaits the in-process worker like any async call.
test Home {
    test "counting shows the total" {
        let screen = render(Home())

        screen.button(named: "Count below 100").click()
        await screen.settle()

        expect screen.text("25 primes below 100").isVisible
    }

    test "a count the worker refuses says why" {
        let screen = render(Home())

        screen.button(named: "Count too many").click()
        await screen.settle()

        expect screen.text("At most 100000000").isVisible
    }

    test "a crash is a typed failure the page handles" {
        let screen = render(Home())

        screen.button(named: "Crash").click()
        await screen.settle()

        expect screen.text("The worker crashed.").isVisible
    }
}
