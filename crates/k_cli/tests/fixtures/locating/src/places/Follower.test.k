import k.device.Position
import k.test.TestGeolocation

test Follower {
    @Inject let geolocation: TestGeolocation

    test "a watch before access is granted is not asked" {
        let screen = render(Follower())
        await screen.settle()

        expect screen.text("Status not asked").isVisible
    }

    test "a watch yields the current position, then each move" {
        geolocation.setPermission(.granted)
        geolocation.move(to: Position.sample(latitude: 10.0, longitude: 0.0))
        let screen = render(Follower())
        await screen.settle()

        expect screen.text("Seen 1 positions").isVisible
        expect geolocation.lastAccuracy == .precise

        geolocation.move(to: Position.sample(latitude: 11.0, longitude: 0.0))
        await screen.settle()
        geolocation.move(to: Position.sample(latitude: 12.0, longitude: 0.0))
        await screen.settle()

        expect screen.text("Seen 3 positions").isVisible
        expect screen.text("Last 12.0").isVisible
        expect screen.text("Status following").isVisible
    }

    test "revoking the permission ends the watch, and the effect, run again, finds it denied" {
        geolocation.setPermission(.granted)
        let screen = render(Follower())
        await screen.settle()

        geolocation.setPermission(.denied)
        await screen.settle()

        expect screen.text("Status denied").isVisible
        expect screen.text("Seen 0 positions").isVisible
    }

    test "a permission granted later starts the watch: the effect read it" {
        let screen = render(Follower())
        await screen.settle()

        expect screen.text("Status not asked").isVisible

        geolocation.move(to: Position.sample(latitude: 10.0, longitude: 0.0))
        geolocation.setPermission(.granted)
        await screen.settle()

        expect screen.text("Status following").isVisible
        expect screen.text("Seen 1 positions").isVisible
    }

    test "without geolocation the watch is unsupported" {
        geolocation.setSupported(false)
        let screen = render(Follower())
        await screen.settle()

        expect screen.text("Status unsupported").isVisible
    }
}
