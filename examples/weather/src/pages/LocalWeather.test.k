import testing.FakeWeatherService
import weather.WeatherService

test LocalWeather {
    use FakeWeatherService for WeatherService

    @Inject let weather: FakeWeatherService

    test "the dashboard for a position shows its current conditions and air quality" {
        let screen = render(LocalWeather(latitude: "52.52", longitude: "13.41"))
        await screen.settle()

        expect screen.heading(named: "Your location").isVisible
        expect screen.text("8°C").isVisible
        expect screen.text("Fair").isVisible
        expect weather.forecastCalls == 1
    }

    test "a failure offers to try again" {
        weather.simulateFailure(error: .offline)
        let screen = render(LocalWeather(latitude: "52.52", longitude: "13.41"))
        await screen.settle()

        expect screen.heading(named: "Weather unavailable").isVisible
        expect screen.button(named: "Try again").isVisible
    }

    test "coordinates that aren't numbers fail the load" {
        let screen = render(LocalWeather(latitude: "north", longitude: "13.41"))
        await screen.settle()

        expect screen.heading(named: "Weather unavailable").isVisible
        expect weather.forecastCalls == 0
    }
}
