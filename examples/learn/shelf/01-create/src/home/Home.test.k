import k.test.render

test Home {
    test "the start page shows the app's name" {
        let screen = render(Home())

        expect screen.heading(named: "Shelf").isVisible
    }
}
