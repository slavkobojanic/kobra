import k.test.TestAppUpdates

test UpdateBanner {
    @Inject let updates: TestAppUpdates

    test "no banner until a newer build arrives" {
        let screen = render(UpdateBanner())

        expect not screen.button(named: "Reload").exists

        updates.announce()

        expect screen.text("A new version is ready.").isVisible
        screen.button(named: "Reload").click()

        expect updates.applied == 1
    }
}
