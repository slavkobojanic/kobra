import testing.FakeWeatherService
import weather.RecentPlaces
import weather.WeatherService

test Home {
    use FakeWeatherService for WeatherService

    @Inject let recent: RecentPlaces

    test "the start page offers search and featured cities" {
        let screen = render(Home())

        expect screen.heading(named: "Weather").isVisible
        expect screen.textField(labeled: "City").isVisible
        expect screen.link(named: "Berlin").isVisible
        expect screen.link(named: "São Paulo").isVisible
    }

    test "places viewed before are listed, in the browser" {
        recent.remember(place: .lisbon)

        let screen = render(Home())

        expect screen.heading(named: "Recently viewed").isVisible
        expect screen.link(named: "Lisbon, Portugal").isVisible
    }

    test "with no places viewed, there is no recent list" {
        let screen = render(Home())

        expect not screen.heading(named: "Recently viewed").isVisible
    }
}
