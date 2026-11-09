import k.device.Position
import k.test.TestGeolocation

test UseMyLocation {
    @Inject let geolocation: TestGeolocation

    test "the first click asks, then opens the weather at the device's position" {
        geolocation.move(to: Position.sample(latitude: 52.52, longitude: 13.41))
        let screen = render(UseMyLocation())

        screen.button(named: "Use my location").click()
        await screen.settle()

        expect geolocation.prompts == 1
        expect geolocation.lastAccuracy == .coarse
        expect screen.currentPage == .localWeather(latitude: "52.52", longitude: "13.41")
    }

    test "a refused prompt says so, and the button goes" {
        geolocation.answerPrompts(with: .denied)
        let screen = render(UseMyLocation())

        screen.button(named: "Use my location").click()
        await screen.settle()

        expect screen.text("Location access is off for this site. Search for a city instead.").isVisible
        expect not screen.button(named: "Use my location").exists
    }

    test "a device with no fix says so" {
        geolocation.setPermission(.granted)
        let screen = render(UseMyLocation())

        screen.button(named: "Use my location").click()
        await screen.settle()

        expect geolocation.prompts == 0
        expect screen.text("Your location couldn't be found. Search for a city instead.").isVisible
    }

    test "a blocked location, or a browser that can't locate, shows no button" {
        geolocation.setPermission(.denied)

        expect not render(UseMyLocation()).button(named: "Use my location").exists

        geolocation.setPermission(.prompt)
        geolocation.setSupported(false)

        expect not render(UseMyLocation()).button(named: "Use my location").exists
    }
}
