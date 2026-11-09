import k.test.TestConnectivity

test OfflineBanner {
    @Inject let connectivity: TestConnectivity

    test "online, there is no banner" {
        let screen = render(OfflineBanner())

        expect connectivity.isOnline
        expect not screen.text("You're offline").exists
    }

    test "going offline shows the banner, and coming back hides it" {
        let screen = render(OfflineBanner())

        connectivity.goOffline()

        expect screen.status().text == "You're offline"

        connectivity.goOnline()

        expect not screen.text("You're offline").exists
    }
}
