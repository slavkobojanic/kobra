# Weather: a K example app

A small weather dashboard written entirely in K. Search for a city, open its dashboard, and see current conditions, air quality and a 24-hour temperature chart. The data comes from the free [Open-Meteo](https://open-meteo.com) APIs, which need no API key, and refreshes every ten minutes while the page is open.

The app has about 880 lines of K, not counting blank lines, comments, tests and test fakes. It is meant to be read: every file shows one or two language features in a realistic setting.

> The K compiler doesn't exist yet, so this code hasn't been compiled. It follows the decisions in [`docs/design`](../../docs/design/README.md) (the ones it leans on most are listed [below](#design-documents-this-example-relies-on)) and the syntax in [`docs/compiler/grammar.md`](../../docs/compiler/grammar.md).

## What it demonstrates

| Feature | Where |
|---|---|
| Pages with `@Param`: a required path parameter and an optional enum query parameter | [`pages/CityWeather.k`](src/pages/CityWeather.k) |
| `@Load` fields with `loading` and `failed` blocks | [`pages/CityWeather.k`](src/pages/CityWeather.k) |
| Loading ordered by dependencies: `place` first, then `forecast` and `airQuality` in parallel | [`pages/CityWeather.k`](src/pages/CityWeather.k), tested in [`CityWeather.test.k`](src/pages/CityWeather.test.k) |
| `LoadState` with an exhaustive `match` in a view, including a `where` guard | [`components/CitySearch.k`](src/components/CitySearch.k) |
| An async `@Effect` that is cancelled automatically, used as a debounce with `clock.sleep` | [`components/CitySearch.k`](src/components/CitySearch.k) |
| Typed errors: `throws WeatherError`, `try`, exhaustive `catch` with patterns, translating `HttpError` at the service boundary without Open-Meteo's response body | [`weather/WeatherError.k`](src/weather/WeatherError.k), [`weather/WeatherService.k`](src/weather/WeatherService.k) |
| Typed HTTP statuses: `HttpStatus` cases as nested patterns (`catch .upstream(.badRequest), .upstream(.notFound)`, `expect throws WeatherError.upstream(.serviceUnavailable)`), and `status.isServerError` in a `where` guard | [`weather/WeatherService.k`](src/weather/WeatherService.k), [`weather/WeatherService.test.k`](src/weather/WeatherService.test.k), [`pages/CityWeather.k`](src/pages/CityWeather.k) |
| Exhaustive `match` over the error inside `failed`, with `metadata { status(.notFound) }` so a missing place answers 404. The page's own `metadata` isn't evaluated when `failed` renders | [`pages/CityWeather.k`](src/pages/CityWeather.k) |
| Condition lists in `if let` and `guard let` (`guard let index = europeanIndex, index >= 0 else`) | [`weather/AirQuality.k`](src/weather/AirQuality.k), [`weather/Place.k`](src/weather/Place.k) |
| `match` on integers and ranges with `default` | [`weather/WeatherCondition.k`](src/weather/WeatherCondition.k), [`weather/AirQuality.k`](src/weather/AirQuality.k) |
| Unit literals and `Duration`: `10.minutes`, `300ms`, `10.seconds`, `180deg`, `240px` | [`pages/CityWeather.k`](src/pages/CityWeather.k), [`components/CitySearch.k`](src/components/CitySearch.k), [`components/CurrentConditionsCard.k`](src/components/CurrentConditionsCard.k) |
| The injected `Clock`: a periodic refresh with `clock.every`, cancelled in `onCleanup` | [`pages/CityWeather.k`](src/pages/CityWeather.k) |
| A relative time against the injected clock: `updatedAt.formatted(as: .relative, from: clock.now)` | [`components/CurrentConditionsCard.k`](src/components/CurrentConditionsCard.k) |
| A service wrapping an external HTTP API with `k.http`'s `HttpClient` | [`weather/WeatherService.k`](src/weather/WeatherService.k) |
| Caching in a service (places), and deliberately not caching (forecasts) | [`weather/WeatherService.k`](src/weather/WeatherService.k) |
| Mapping a wire format to domain types, with `@JsonKey` and validation | [`weather/OpenMeteo.k`](src/weather/OpenMeteo.k) |
| A `k.charts` line chart themed by tokens, with the library's `chartGrid` and `chartAxis` overridden by assignment | [`components/TemperatureChart.k`](src/components/TemperatureChart.k), [`theme/WeatherTheme.k`](src/theme/WeatherTheme.k) |
| A `theme` with a `dark` variant that follows the operating system (`matches .prefersDark`), and token categories (`color`, `space`, `radius`, `fontSize`) | [`theme/WeatherTheme.k`](src/theme/WeatherTheme.k) |
| Reactive style values (`{windArrowAngle}`, `{badgeColor}`) and conditional classes (`.selected when …`) | [`components/CurrentConditionsCard.k`](src/components/CurrentConditionsCard.k), [`components/AirQualityPanel.k`](src/components/AirQualityPanel.k), [`pages/CityWeather.k`](src/pages/CityWeather.k) |
| `metadata` blocks: defaults in a layout, overridden by pages, using `@Load` values | [`layouts/Dashboard.k`](src/layouts/Dashboard.k), [`pages/CityWeather.k`](src/pages/CityWeather.k), [`pages/Home.k`](src/pages/Home.k) |
| Renamed HTML elements: `box`, `text`, `paragraph`, `heading`, `list` / `item`, `link`, `definitionList` / `term` / `definition`, `figure` / `caption` | throughout `components/` and `pages/` |
| Typed navigation: `link(to: .cityWeather(placeId: id).units(.imperial))`, and a named URL (`page Home at "/"`) | [`pages/CityWeather.k`](src/pages/CityWeather.k), [`pages/Home.k`](src/pages/Home.k) |
| `App.k`: the app-wide default layout and metadata | [`App.k`](App.k) |
| Tests that inject fakes with `use X for Y`: an app-written `HttpClient` that can also simulate timeouts and dropped connections, and a fake `WeatherService` for pages and components | [`weather/WeatherService.test.k`](src/weather/WeatherService.test.k), [`pages/CityWeather.test.k`](src/pages/CityWeather.test.k), [`components/CitySearch.test.k`](src/components/CitySearch.test.k) |
| Fixtures as static members of `public extend` blocks in a test-only module (`.berlin`, `Forecast.sample(units:)`) | [`testing/Samples.k`](src/testing/Samples.k) |
| Virtual time with the default `TestClock`, reached with `@Inject` and no `use` line (debounce, parallel loading, the 10-minute refresh) | [`components/CitySearch.test.k`](src/components/CitySearch.test.k), [`pages/CityWeather.test.k`](src/pages/CityWeather.test.k) |
| HTTP-level tests with `TestClient` and `PublicUrls`: the 404 status and `<title>` that crawlers receive | [`pages/CityWeather.test.k`](src/pages/CityWeather.test.k) |

## Structure

Each directory under `src/` is a module. Each file has at most one `public` declaration, named after the file. Members follow their type: the fields, computed properties and methods of a `public` struct, enum or service are public unless marked `private`, so they carry no modifier ([16-modules.md](../../docs/design/16-modules.md#visibility-of-members)). The wire format in `OpenMeteo.k` has no `public` declaration at all, so it stays inside the `weather` module.

```
weather/
├── k.toml                         manifest (24-projects.md)
├── App.k                          the app: default layout, app-wide metadata
└── src/
    ├── theme/
    │   └── WeatherTheme.k         design tokens + dark variant (follows the OS)
    ├── weather/                   domain types and the only code that talks to the network
    │   ├── Place.k                a location; displayName
    │   ├── Forecast.k             current conditions + 24 hours, in the place's time zone
    │   ├── CurrentConditions.k
    │   ├── HourlyReading.k
    │   ├── WeatherCondition.k     WMO weather codes → a small enum (+ icons)
    │   ├── AirQuality.k           European AQI → AirQualityCategory
    │   ├── AirQualityCategory.k
    │   ├── UnitSystem.k           metric / imperial, with formatting
    │   ├── WeatherError.k         the service's typed error (no upstream bodies)
    │   ├── WeatherService.k       Open-Meteo client: search, place, forecast, air quality
    │   ├── OpenMeteo.k            package-private wire format and conversion
    │   └── *.test.k
    ├── components/
    │   ├── CitySearch.k           type-ahead search (LoadState, async effect)
    │   ├── CurrentConditionsCard.k
    │   ├── AirQualityPanel.k
    │   ├── TemperatureChart.k     k.charts line chart
    │   └── CitySearch.test.k
    ├── layouts/
    │   └── Dashboard.k            header, content, Open-Meteo attribution
    ├── pages/
    │   ├── Home.k                 "/" — search + featured cities
    │   ├── CityWeather.k          the dashboard for one place
    │   └── *.test.k
    └── testing/                   test-only module (21-testing.md)
        ├── FakeHttpClient.k       canned HTTP responses and failures, records requests
        ├── FakeWeatherService.k   in-memory WeatherService with latency and failures
        └── Samples.k              .berlin, .lisbon, Forecast.sample(units:), AirQuality.sample
```

The layering follows [10-services.md](../../docs/design/10-services.md). Pages and components inject `WeatherService`, and the stdlib `Clock` service where they need timers or the current time. Only `WeatherService` injects `HttpClient`. Open-Meteo's JSON never leaves the `weather` module.

### How a dashboard loads

```k
@Load let place: Place = try await weather.place(id: placeId)
@Load let forecast: Forecast = try await weather.forecast(place: place, units: units)
@Load let airQuality: AirQuality = try await weather.airQuality(place: place)
```

`forecast` and `airQuality` read `place`, so the compiler starts them when `place` resolves, and runs them in parallel with each other. `loading` renders until all three have resolved. `failed` receives a `WeatherError`, and the page matches on it exhaustively: not found (which also sets `status(.notFound)`), or one of offline, timeout, an upstream error status and an invalid response, each explained in its own sentence with a retry button. The page's own `metadata` block reads `place`, so it isn't evaluated when `failed` renders: the not-found case sets its own title, and the other failures keep the layout's. Every ten minutes `reload()` re-runs the graph. The cached place returns at once, and the dashboard stays on screen until the new forecast arrives.

A failed load rendered on the server is sent to the browser for hydration, so `WeatherError` never carries Open-Meteo's response body. `WeatherService` keeps only the `HttpStatus` (`case upstream(HttpStatus)`), as [32-http-client.md](../../docs/design/32-http-client.md#errors) recommends.

## Running it

These commands will exist once the toolchain ships ([13-tooling.md](../../docs/design/13-tooling.md)):

```bash
cd examples/weather

k dev       # dev server with state-preserving hot reload
k test      # run every *.test.k file (add --watch to re-run on save)
k build     # dist/app: one self-contained server executable plus client assets
k check     # types, formatting, tests and lockfiles, as CI runs them
```

`k build` also writes `routes.lock`. The routes are `/` (`Home`) and the default URL for `CityWeather`, for example `/city-weather/2950159?units=imperial`.

The app needs outbound HTTPS access to `geocoding-api.open-meteo.com`, `api.open-meteo.com` and `air-quality-api.open-meteo.com`. Requests are made from the browser, and from the server during server rendering. Open-Meteo's free tier is for non-commercial use, and its data is licensed under CC BY 4.0. The layout shows the attribution the licence requires.

## Tests

| File | Covers |
|---|---|
| `weather/WeatherService.test.k` | Mapping search, place, forecast and air-quality responses. Query parameters per unit system. The two-character minimum. Place caching, and forecasts not being cached. Errors: not found (400 and 404), a server error during a lookup that isn't mistaken for not found, server error (503), timeout, offline, malformed JSON, series of different lengths, an unknown time zone, and a missing AQI. |
| `weather/WeatherCondition.test.k` | WMO code groups, showers, unknown and negative codes, day and night icons |
| `weather/AirQuality.test.k` | Every category boundary, values above the scale, missing and negative indexes |
| `weather/Place.test.k` | `displayName` with repeated or missing parts |
| `components/CitySearch.test.k` | The idle hint, the debounce (299 ms vs 300 ms, and restarting on new input), the one-letter minimum, the in-progress text, no matches, failure, and clearing the field |
| `pages/CityWeather.test.k` | The loaded dashboard, the `loading` block, parallel loading after the place, imperial units from the parameter, not found (rendered, and as a server-rendered 404 with its title), a server-rendered 200, offline, timeout with a working retry, 5xx vs 4xx messages, an invalid response, refreshing every 10 minutes, keeping the view during a refresh, and stopping the timer on unmount |
| `pages/Home.test.k` | The search field and featured links |

## Design documents this example relies on

Besides the core topics (components, views, styles, reactivity, routing), the example leans on these decisions:

| Document | Used for |
|---|---|
| [`39-doc-comments.md`](../../docs/design/39-doc-comments.md) | `///` doc comments with `- Parameter:`, `- Returns:`, `- Throws:` sections. References resolve against the file's imports, the app and fully qualified names, so a name the file doesn't import is written in full (`k.http.HttpError`) |
| [`24-projects.md`](../../docs/design/24-projects.md) | [`k.toml`](k.toml) and the fixed project layout |
| [`23-app-structure.md`](../../docs/design/23-app-structure.md) | [`App.k`](App.k): `public app`, `@Layout` on the app, app-wide `metadata` |
| [`16-modules.md`](../../docs/design/16-modules.md) | Members that follow their type's visibility, `public extend`, the test-only `testing` module |
| [`32-http-client.md`](../../docs/design/32-http-client.md) | `HttpClient`, `HttpRequest`, `HttpResponse`, `HttpStatus` and its cases as patterns, `HttpError`, `fetchJson`, `@JsonKey` |
| [`09-errors.md`](../../docs/design/09-errors.md) | Typed throws, exhaustive `catch`, translating errors at a layer boundary |
| [`14-async-data.md`](../../docs/design/14-async-data.md) | `@Load`, `loading`, `failed`, `reload()`, keeping the view during a refresh, `status(.notFound)`, the page `metadata` skipped when `failed` renders |
| [`07-themes-and-units.md`](../../docs/design/07-themes-and-units.md) | The theme, `variant dark matches .prefersDark`, overriding library tokens by assignment |
| [`37-charts.md`](../../docs/design/37-charts.md) | `LineChart`, `LineSeries`, `ChartPoint`, chart tokens |
| [`35-time.md`](../../docs/design/35-time.md) | The injected `Clock`: `clock.every`, `clock.sleep`, `clock.now` in relative times; `TimerHandle.cancel()`, `TestClock` |
| [`21-testing.md`](../../docs/design/21-testing.md) | Test groups, `service FakeX implements X`, `@Inject` of a fake, hermetic defaults, `screen.settle()`, `fill`, `unmount`, `TestClient` |

A few smaller stdlib APIs are assumed and not specified anywhere yet:

- `formatted(as: .wholeNumber)` beside the documented `.currency` and `.relative`
- `Date(unixSeconds:)`, and `TimeZone.parse(_:)` following the `Type.parse` convention in [20-standard-library.md](../../docs/design/20-standard-library.md#literals-and-parsing)
- `Angle(degrees:)`
- `String.trimmed()`
- `Icon(name:)` with Lucide-style icon names
