import k.device.Position
import k.test.TestGeolocation

test UseMyLocation {
    @Inject let geolocation: TestGeolocation
    @Inject let places: PlaceService

    test "the first click asks, then selects the place at the device's position" {
        geolocation.move(to: Position.sample(latitude: 52.52, longitude: 13.40))
        let screen = render(UseMyLocation())

        screen.button(named: "Use my location").click()
        await screen.settle()

        expect geolocation.prompts == 1
        expect geolocation.permission == .granted
        expect places.latitude == 52.52
        expect screen.text("At 52.52, 13.4").isVisible
    }

    test "a second click doesn't ask again" {
        geolocation.move(to: Position.sample(latitude: 1.5, longitude: 2.5))
        let screen = render(UseMyLocation())

        screen.button(named: "Use my location").click()
        await screen.settle()
        screen.button(named: "Use my location").click()
        await screen.settle()

        expect geolocation.prompts == 1
        expect geolocation.lastAccuracy == .coarse
    }

    test "a blocked location hides the button" {
        geolocation.setPermission(.denied)

        let screen = render(UseMyLocation())

        expect not screen.button(named: "Use my location").exists
    }

    test "a browser without geolocation hides the button" {
        geolocation.setSupported(false)

        let screen = render(UseMyLocation())

        expect not screen.button(named: "Use my location").exists
    }

    test "refusing the prompt says so, and the button goes" {
        geolocation.answerPrompts(with: .denied)
        let screen = render(UseMyLocation())

        screen.button(named: "Use my location").click()
        await screen.settle()

        expect geolocation.prompts == 1
        expect screen.text("Location access was refused").isVisible
        expect not screen.button(named: "Use my location").exists
    }

    test "a device with no fix says so" {
        let screen = render(UseMyLocation())

        screen.button(named: "Use my location").click()
        await screen.settle()

        expect screen.text("No position could be found").isVisible
    }

    test "the button follows a permission changed in the site's settings" {
        let screen = render(UseMyLocation())

        expect screen.button(named: "Use my location").exists

        geolocation.setPermission(.denied)
        await screen.settle()

        expect not screen.button(named: "Use my location").exists
    }
}
