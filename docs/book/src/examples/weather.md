# Weather: loading data from an HTTP API

`examples/weather` is a weather dashboard backed by the free [Open-Meteo](https://open-meteo.com) APIs,
which need no API key. You search for a city, open its dashboard and see current conditions, air
quality and a 24-hour temperature chart, refreshed every ten minutes while the page is open.

It is the example for the data-loading half of K: a service that wraps an external HTTP API, typed
errors from the network to the page, pages with parameters and `@Load`, effects driven by the injected
clock, a chart, and tests that fake the HTTP client and the clock.

```bash
k test examples/weather
k dev examples/weather          # http://localhost:8080
```

## The layout

```text
weather/
├── App.k                   the default layout and app-wide metadata
└── src/
    ├── theme/              WeatherTheme: tokens, a dark variant that follows the OS
    ├── weather/            domain types and the only code that talks to the network:
    │                       Place, Forecast, AirQuality, UnitSystem, WeatherCondition,
    │                       WeatherError, OpenMeteo (the wire format), WeatherService
    ├── components/         CitySearch, CurrentConditionsCard, AirQualityPanel, TemperatureChart
    ├── layouts/            Dashboard
    ├── pages/              Home (search) and CityWeather (the dashboard)
    └── testing/            FakeHttpClient, FakeWeatherService, Samples
```

```k
// examples/weather/App.k
@Layout(Dashboard)
public app Weather {
    metadata {
        manifest(name: "Weather", shortName: "Weather")
        themeColor(.primary)
        locale(.english)
    }
}
```

## The network boundary

`WeatherService` is the only code that sends requests. It injects the platform `HttpClient` and maps
every `HttpError` to the app's own `WeatherError`, dropping Open-Meteo's response body
([Calling HTTP APIs](../apps/http.md)):

```k
// examples/weather/src/weather/WeatherService.k
private func fetch<T>(url: Url, query: Map<String, String>) async throws WeatherError -> T where T implements Codable {
    let request = HttpRequest(method: .get, url: url)
        .query(query)
        .timeout(requestTimeout)

    do {
        return try await http.fetchJson(request: request)
    } catch .offline {
        throw WeatherError.offline
    } catch .timeout {
        throw WeatherError.timeout
    } catch .status(let status, _) {
        throw WeatherError.upstream(status)
    } catch .decoding(let message) {
        throw WeatherError.invalidResponse(reason: message)
    }
}
```

`OpenMeteo.k` declares the wire format as module-private structs with `@JsonKey` names, so the rest of
the app sees only domain types such as `Forecast` and `Place`.

## A page that loads in dependency order

```k
// examples/weather/src/pages/CityWeather.k
public page CityWeather {
    // ...
    @Param let placeId: Int
    // ...
    @Param let units: UnitSystem = .metric

    @Inject let weather: WeatherService
    @Inject let clock: Clock
    @Inject let recent: RecentPlaces

    @Load(keepOnReload: true) let place: Place = try await weather.place(id: placeId)
    @Load let forecast: Forecast = try await weather.forecast(place: place, units: units)
    @Load let airQuality: AirQuality = try await weather.airQuality(place: place)
    // ...
```

- `placeId` has no default, so it is a path parameter; `units` has one, so it is an optional query
  parameter: `link(to: .cityWeather(placeId: id).units(.imperial))` ([Pages and routing](../apps/routing.md)).
- `forecast` and `airQuality` read `place`, so they start when it has loaded, and run in parallel with
  each other ([Async data loading](../apps/async-data.md)).
- The view sees plain, loaded values; `loading` and `failed` blocks cover the rest.

The refresh is an effect with a timer that is cancelled when the page goes away:

```k
// examples/weather/src/pages/CityWeather.k
@Effect(on: [placeId])
func refreshPeriodically() {
    let timer = clock.every(refreshInterval) { reload() }

    onCleanup { timer.cancel() }
}
```

`reload()` is generated for every declaration with `@Load` fields. `place` is marked
`@Load(keepOnReload: true)`: a place never changes, so a refresh keeps it and fetches only the forecast and
the air quality, even right after the server rendered the page.

### Failures are part of the page

`failed` matches the error exhaustively. A missing place answers `404` through `metadata`, so crawlers
and the HTTP-level tests see the right status:

```k
// examples/weather/src/pages/CityWeather.k
failed(let error) {
    section {
        match error {
            case .placeNotFound {
                metadata {
                    title("Place not found")
                    status(.notFound)
                }

                heading(level: 1) { "Place not found" }

                paragraph { explain(error: error) }

                link(to: .home) { "Search for a place" }
            }

            case .offline, .timeout, .upstream, .invalidResponse {
                heading(level: 1) { "Weather unavailable" }

                paragraph { explain(error: error) }

                button { "Try again" }
                    .on(click: reload)
            }
        }
    }
    .class(.status)
}
```

## A debounced search

`CitySearch` searches as you type. Its effect is `async`, and every keystroke re-runs it, which cancels
the run that is waiting in `clock.sleep`, so the request goes out only once typing pauses:

```k
// examples/weather/src/components/CitySearch.k
@Effect
func search() async {
    let text = query.trimmed()

    guard text.count >= weather.minimumQueryLength else {
        results = .idle
        return
    }

    await clock.sleep(debounceDelay)

    results = .loading

    do {
        let places = try await weather.searchPlaces(query: text)
        results = .loaded(places)
    } catch let error {
        results = .failed(error)
    }
}
```

The view matches the `LoadState`, with a `where` guard for an empty result:

```k
// examples/weather/src/components/CitySearch.k
case .loaded(let places) where places.isEmpty {
    paragraph { "No places match “{query.trimmed()}”." }
        .class(.hint)
}
```

## A list only the browser knows

The start page lists the places opened recently. They are kept in local storage by `RecentPlaces`, a
client-only service, so the server can't render them. Only that list renders in the browser; the rest of
the start page is still rendered on the server:

```k
// examples/weather/src/components/RecentPlacesList.k
@Rendering(.client)
public component RecentPlacesList {
    @Inject let recent: RecentPlaces
// ...
    loading {
    }
```

The server renders the empty `loading` block in its place, and the browser swaps in the list right after
hydration, once the service's `init()` has read local storage. The list is the last thing on the page, so
nothing the server rendered moves when it appears ([Components](../apps/components.md#rendering-in-the-browser-only)).

## The theme and the chart

```k
// examples/weather/src/theme/WeatherTheme.k
public theme WeatherTheme {
    color primary: Color = #2563EB
    // ...
    color chartTemperature: Color = #EA580C
    color chartFeelsLike: Color = #2563EB

    chartGrid = #E2E8F0
    chartAxis = #64748B
    // ...
    variant dark matches .prefersDark {
        surface = #0F172A
        surfaceRaised = #1E293B
        text = #F1F5F9
        muted = #94A3B8
        border = #334155
        chartGrid = #334155
        chartAxis = #94A3B8
    }
}
```

`chartGrid` and `chartAxis` are tokens `k.charts` declares; the app overrides them with the assignment
form. `variant dark matches .prefersDark` applies when the operating system prefers dark mode, until
the `Theme` service is told otherwise. [Charts](../apps/charts.md) walks through `TemperatureChart`.

## Testing with fakes and virtual time

Pages are tested against a `FakeWeatherService`; the service itself against a `FakeHttpClient` that can
also simulate timeouts and dropped connections. Time is the default `TestClock`, so the ten-minute
refresh takes no real time:

```k
// examples/weather/src/pages/CityWeather.test.k
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
    // ...
}
```

The service's own tests pin the error mapping:

```k
// examples/weather/src/weather/WeatherService.test.k
test "a server error status is an upstream error" {
    http.serveText(path: "/v1/forecast", body: "Service unavailable", status: .serviceUnavailable)

    expect throws WeatherError.upstream(.serviceUnavailable) {
        try await weather.forecast(place: .berlin, units: .metric)
    }
}
```
