import testing.FakeWeatherService
import weather.WeatherService

test Home {
    use FakeWeatherService for WeatherService

    test "the start page offers search and featured cities" {
        let screen = render(Home())

        expect screen.heading(named: "Weather").isVisible
        expect screen.textField(labeled: "City").isVisible
        expect screen.link(named: "Berlin").isVisible
        expect screen.link(named: "São Paulo").isVisible
    }
}
