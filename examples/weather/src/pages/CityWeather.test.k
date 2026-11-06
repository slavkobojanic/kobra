import k.http.HttpRequest
import k.server.PublicUrls
import k.test.TestClient
import k.test.TestClock
import testing.FakeWeatherService
import weather.Place
import weather.WeatherService

test CityWeather {
    use FakeWeatherService for WeatherService

    @Inject let weather: FakeWeatherService
    @Inject let clock: TestClock
    @Inject let client: TestClient
    @Inject let urls: PublicUrls

    // Loading

    test "the dashboard shows the place, current conditions and air quality" {
        let screen = render(CityWeather(placeId: Place.berlin.id))
        await screen.settle()

        expect screen.heading(named: "Berlin, Land Berlin, Germany").isVisible
        expect screen.text("8°C").isVisible
        expect screen.text("Partly cloudy").isVisible
        expect screen.text("Fair").isVisible
        expect screen.text("Next 24 hours").isVisible
    }

    test "the loading block shows until every load has finished" {
        weather.simulateLatency(duration: 1.second)
        let screen = render(CityWeather(placeId: Place.berlin.id))
        await screen.settle()

        expect screen.text("Loading forecast…").isVisible

        clock.advance(by: 1.second)
        await screen.settle()

        expect screen.text("Loading forecast…").isVisible

        clock.advance(by: 1.second)
        await screen.settle()

        expect screen.heading(named: "Berlin, Land Berlin, Germany").isVisible
    }

    test "forecast and air quality wait for the place, then load in parallel" {
        weather.simulateLatency(duration: 1.second)
        let screen = render(CityWeather(placeId: Place.berlin.id))
        await screen.settle()

        expect weather.forecastCalls == 0
        expect weather.airQualityCalls == 0

        clock.advance(by: 1.second)
        await screen.settle()

        expect weather.forecastCalls == 1
        expect weather.airQualityCalls == 1

        clock.advance(by: 1.second)
        await screen.settle()

        expect screen.text("8°C").isVisible
    }

    test "imperial units come from the page parameter" {
        let screen = render(CityWeather(placeId: Place.berlin.id).units(.imperial))
        await screen.settle()

        expect screen.text("46°F").isVisible
    }

    // Failures

    test "an unknown place shows a not-found message with a way back" {
        let screen = render(CityWeather(placeId: 42))
        await screen.settle()

        expect screen.heading(named: "Place not found").isVisible
        expect screen.text("There is no place with id 42. Try searching for it instead.").isVisible
        expect screen.link(named: "Search for a place").isVisible
        expect not screen.button(named: "Try again").exists
    }

    test "a server-rendered unknown place answers 404, not a soft 200" {
        let request = HttpRequest(method: .get, url: urls.absolute(.cityWeather(placeId: 42)))

        let response = await client.send(request: request)

        expect response.status == .notFound
        expect response.body.contains("<title>Place not found</title>")
    }

    test "a server-rendered dashboard answers 200 with the place in its title" {
        let request = HttpRequest(method: .get, url: urls.absolute(.cityWeather(placeId: Place.berlin.id)))

        let response = await client.send(request: request)

        expect response.status == .ok
        expect response.body.contains("<title>Berlin weather</title>")
    }

    test "being offline is explained, with a way to retry" {
        weather.simulateFailure(error: .offline)
        let screen = render(CityWeather(placeId: Place.berlin.id))
        await screen.settle()

        expect screen.heading(named: "Weather unavailable").isVisible
        expect screen.text("You appear to be offline.").isVisible
        expect screen.button(named: "Try again").isVisible
    }

    test "a timeout explains itself and can be retried" {
        weather.simulateFailure(error: .timeout)
        let screen = render(CityWeather(placeId: Place.berlin.id))
        await screen.settle()

        expect screen.text("Open-Meteo took too long to respond.").isVisible

        weather.clearFailure()
        screen.button(named: "Try again").click()
        await screen.settle()

        expect screen.text("8°C").isVisible
    }

    test "server errors and client errors are explained differently" {
        weather.simulateFailure(error: .upstream(.serviceUnavailable))
        let serverError = render(CityWeather(placeId: Place.berlin.id))
        await serverError.settle()

        expect serverError.text("Open-Meteo is having problems (error 503).").isVisible

        weather.simulateFailure(error: .upstream(.tooManyRequests))
        let clientError = render(CityWeather(placeId: Place.berlin.id))
        await clientError.settle()

        expect clientError.text("Open-Meteo could not answer this request (error 429).").isVisible
    }

    test "an unreadable response shows a generic message" {
        weather.simulateFailure(error: .invalidResponse(reason: "hourly series have different lengths"))
        let screen = render(CityWeather(placeId: Place.berlin.id))
        await screen.settle()

        expect screen.text("Open-Meteo sent an unexpected response. Please try again later.").isVisible
        expect screen.button(named: "Try again").isVisible
    }

    // Refreshing

    test "the forecast refreshes every ten minutes" {
        let screen = render(CityWeather(placeId: Place.berlin.id))
        await screen.settle()

        expect weather.forecastCalls == 1

        clock.advance(by: 9.minutes)
        await screen.settle()

        expect weather.forecastCalls == 1

        clock.advance(by: 1.minute)
        await screen.settle()

        expect weather.forecastCalls == 2
        expect weather.airQualityCalls == 2

        clock.advance(by: 10.minutes)
        await screen.settle()

        expect weather.forecastCalls == 3
    }

    test "a refresh keeps the dashboard on screen while it reloads" {
        let screen = render(CityWeather(placeId: Place.berlin.id))
        await screen.settle()

        weather.simulateLatency(duration: 5.seconds)
        clock.advance(by: 10.minutes)
        await screen.settle()

        expect screen.text("8°C").isVisible
        expect not screen.text("Loading forecast…").isVisible

        clock.advance(by: 10.seconds)
        await screen.settle()

        expect weather.forecastCalls == 2
        expect screen.text("8°C").isVisible
    }

    test "the refresh timer stops when the page is closed" {
        let screen = render(CityWeather(placeId: Place.berlin.id))
        await screen.settle()

        screen.unmount()
        clock.advance(by: 30.minutes)
        await screen.settle()

        expect weather.forecastCalls == 1
    }
}
