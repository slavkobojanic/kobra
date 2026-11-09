import k.test.TestConnectivity

test OfflineBanner {
    @Inject let connectivity: TestConnectivity

    test "online, there is no banner" {
        let screen = render(OfflineBanner())

        expect not screen.status().exists
    }

    test "going offline shows the banner, and coming back online hides it" {
        let screen = render(OfflineBanner())

        connectivity.goOffline()

        expect screen.status().text == "You're offline. K Notes can't save changes until the connection is back."

        connectivity.goOnline()

        expect not screen.status().exists
    }
}
