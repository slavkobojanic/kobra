import k.test.TestAppUpdates

test UpdateNotice {
    @Inject let updates: TestAppUpdates

    test "nothing is shown while the page runs the newest build" {
        let screen = render(UpdateNotice())

        expect not screen.text("A new version of Todos is ready.").exists
        expect not screen.button(named: "Reload").exists
    }

    test "a newer build is offered once it has arrived, and Reload switches to it" {
        let screen = render(UpdateNotice())

        updates.announce()

        expect screen.text("A new version of Todos is ready.").isVisible
        screen.button(named: "Reload").click()

        expect updates.applied == 1
    }
}
