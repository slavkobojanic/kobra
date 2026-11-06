import k.test.TestClock
import k.time.Clock
import testing.FakeWeatherService
import weather.WeatherService

test CitySearch {
    use FakeWeatherService for WeatherService
    use TestClock for Clock

    @Inject let weather: FakeWeatherService
    @Inject let clock: TestClock

    test "an empty field shows a hint instead of results" {
        let screen = render(CitySearch())

        expect screen.text("Type at least two letters to search.").isVisible
        expect weather.searchQueries.isEmpty
    }

    test "matching places appear once the user stops typing" {
        let screen = render(CitySearch())

        screen.textField(labeled: "City").fill("Lis")
        clock.advance(by: 300ms)
        await screen.settle()

        expect screen.link(named: "Lisbon, Portugal").isVisible
    }

    test "nothing is searched until typing pauses for 300 ms" {
        let screen = render(CitySearch())

        screen.textField(labeled: "City").fill("Be")
        clock.advance(by: 299ms)
        await screen.settle()

        expect weather.searchQueries.isEmpty
    }

    test "typing again restarts the wait, so only the final text is searched" {
        let screen = render(CitySearch())

        screen.textField(labeled: "City").fill("Be")
        clock.advance(by: 200ms)
        screen.textField(labeled: "City").fill("Ber")
        clock.advance(by: 200ms)
        await screen.settle()

        expect weather.searchQueries.isEmpty

        clock.advance(by: 100ms)
        await screen.settle()

        expect weather.searchQueries == ["Ber"]
        expect screen.link(named: "Berlin, Land Berlin, Germany").isVisible
    }

    test "a single letter never searches" {
        let screen = render(CitySearch())

        screen.textField(labeled: "City").fill("B")
        clock.advance(by: 1.second)
        await screen.settle()

        expect weather.searchQueries.isEmpty
        expect screen.text("Type at least two letters to search.").isVisible
    }

    test "a slow search shows progress" {
        weather.simulateLatency(duration: 2.seconds)
        let screen = render(CitySearch())

        screen.textField(labeled: "City").fill("Berlin")
        clock.advance(by: 300ms)
        await screen.settle()

        expect screen.text("Searching…").isVisible
    }

    test "a search with no matches says so" {
        let screen = render(CitySearch())

        screen.textField(labeled: "City").fill("Atlantis")
        clock.advance(by: 300ms)
        await screen.settle()

        expect screen.text("No places match “Atlantis”.").isVisible
    }

    test "a failed search shows an error instead of results" {
        weather.simulateFailure(error: .network(.offline))
        let screen = render(CitySearch())

        screen.textField(labeled: "City").fill("Berlin")
        clock.advance(by: 300ms)
        await screen.settle()

        expect screen.text("Search is unavailable right now. Please try again.").isVisible
    }

    test "clearing the field returns to the hint" {
        let screen = render(CitySearch())

        screen.textField(labeled: "City").fill("Berlin")
        clock.advance(by: 300ms)
        await screen.settle()
        screen.textField(labeled: "City").fill("")
        await screen.settle()

        expect screen.text("Type at least two letters to search.").isVisible
    }
}
