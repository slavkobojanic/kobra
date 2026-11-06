# Weather: a K example app

A small weather dashboard written entirely in K. Search for a city, open its dashboard, and see current conditions, air quality and a 24-hour temperature chart. The data comes from the free [Open-Meteo](https://open-meteo.com) APIs, which need no API key, and refreshes every ten minutes while the page is open.

The app has about 880 lines of K, not counting blank lines, comments, tests and test fakes. It is meant to be read: every file shows one or two language features in a realistic setting.

> The K compiler doesn't exist yet, so this code hasn't been compiled. It follows the decisions in [`docs/design`](../../docs/design/README.md). Where those were silent, it follows the proposals listed [below](#design-proposals-this-example-relies-on).

## What it demonstrates

| Feature | Where |
|---|---|
| Pages with `@Param`: a required path parameter and an optional enum query parameter | [`pages/CityWeather.k`](src/pages/CityWeather.k) |
| `@Load` fields with `loading` and `failed` blocks | [`pages/CityWeather.k`](src/pages/CityWeather.k) |
| Loading ordered by dependencies: `place` first, then `forecast` and `airQuality` in parallel | [`pages/CityWeather.k`](src/pages/CityWeather.k), tested in [`CityWeather.test.k`](src/pages/CityWeather.test.k) |
| `LoadState` with an exhaustive `match` in a view, including a `where` guard | [`components/CitySearch.k`](src/components/CitySearch.k) |
| An async `@Effect` that is cancelled automatically, used as a debounce | [`components/CitySearch.k`](src/components/CitySearch.k) |
| Typed errors: `throws WeatherError`, `try`, `catch` with patterns, wrapping `HttpError` at the service boundary | [`weather/WeatherError.k`](src/weather/WeatherError.k), [`weather/WeatherService.k`](src/weather/WeatherService.k) |
| Exhaustive `match` over a nested error inside `failed` | [`pages/CityWeather.k`](src/pages/CityWeather.k) |
| `match` on integers and ranges with `default` | [`weather/WeatherCondition.k`](src/weather/WeatherCondition.k), [`weather/AirQuality.k`](src/weather/AirQuality.k) |
| Unit literals and `Duration`: `10.minutes`, `300ms`, `10.seconds`, `180deg`, `240px` | [`pages/CityWeather.k`](src/pages/CityWeather.k), [`components/CitySearch.k`](src/components/CitySearch.k), [`components/CurrentConditionsCard.k`](src/components/CurrentConditionsCard.k) |
| A periodic refresh with `Timer.every`, cancelled in `onCleanup` | [`pages/CityWeather.k`](src/pages/CityWeather.k) |
| A service wrapping an external HTTP API with `k.http`'s `HttpClient` | [`weather/WeatherService.k`](src/weather/WeatherService.k) |
| Caching in a service (places), and deliberately not caching (forecasts) | [`weather/WeatherService.k`](src/weather/WeatherService.k) |
| Mapping a wire format to domain types, with `@JsonKey` and validation | [`weather/OpenMeteo.k`](src/weather/OpenMeteo.k) |
| A `k.charts` line chart themed by tokens | [`components/TemperatureChart.k`](src/components/TemperatureChart.k), [`theme/WeatherTheme.k`](src/theme/WeatherTheme.k) |
| A `theme` with a `dark` variant, and token categories (`color`, `space`, `radius`, `fontSize`) | [`theme/WeatherTheme.k`](src/theme/WeatherTheme.k) |
| Reactive style values (`{windArrowAngle}`, `{badgeColor}`) and conditional classes (`.selected when …`) | [`components/CurrentConditionsCard.k`](src/components/CurrentConditionsCard.k), [`components/AirQualityPanel.k`](src/components/AirQualityPanel.k), [`pages/CityWeather.k`](src/pages/CityWeather.k) |
| `metadata` blocks: defaults in a layout, overridden by pages, using `@Load` values | [`layouts/Dashboard.k`](src/layouts/Dashboard.k), [`pages/CityWeather.k`](src/pages/CityWeather.k), [`pages/Home.k`](src/pages/Home.k) |
| Renamed HTML elements: `box`, `text`, `paragraph`, `heading`, `list` / `item`, `link`, `definitionList` / `term` / `definition`, `figure` / `caption` | throughout `components/` and `pages/` |
| Typed navigation: `link(to: .cityWeather(placeId: id).units(.imperial))`, and a named URL (`page Home at "/"`) | [`pages/CityWeather.k`](src/pages/CityWeather.k), [`pages/Home.k`](src/pages/Home.k) |
| `App.k`: the app-wide default layout and metadata | [`App.k`](App.k) |
| Tests that inject fakes with `use X for Y`: a fake `HttpClient` for the service, and a fake `WeatherService` for pages and components | [`weather/WeatherService.test.k`](src/weather/WeatherService.test.k), [`pages/CityWeather.test.k`](src/pages/CityWeather.test.k), [`components/CitySearch.test.k`](src/components/CitySearch.test.k) |
| Virtual time in tests with `TestClock` (debounce, parallel loading, the 10-minute refresh) | [`components/CitySearch.test.k`](src/components/CitySearch.test.k), [`pages/CityWeather.test.k`](src/pages/CityWeather.test.k) |

## Structure

Each directory under `src/` is a module. Each file has at most one `public` declaration, named after the file. Code in other modules reads only `public` members, so struct fields and service methods that pages use are marked `public`.

```
weather/
├── k.toml                         manifest (proposal: k-toml.md)
├── App.k                          the app: default layout, app-wide metadata
└── src/
    ├── theme/
    │   └── WeatherTheme.k         design tokens + dark variant
    ├── weather/                   domain types and the only code that talks to the network
    │   ├── Place.k                a location; displayName
    │   ├── Forecast.k             current conditions + 24 hours, in the place's time zone
    │   ├── CurrentConditions.k
    │   ├── HourlyReading.k
    │   ├── WeatherCondition.k     WMO weather codes → a small enum (+ icons)
    │   ├── AirQuality.k           European AQI → AirQualityCategory
    │   ├── AirQualityCategory.k
    │   ├── UnitSystem.k           metric / imperial, with formatting
    │   ├── WeatherError.k         the service's typed error
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
    └── testing/                   test-only module (proposal: testing-support.md)
        ├── FakeHttpClient.k       canned HTTP responses, records requests
        ├── FakeWeatherService.k   in-memory WeatherService with latency and failures
        └── Samples.k              .berlin, .lisbon, Forecast.sample(units:), AirQuality.sample
```

The layering follows [10-services.md](../../docs/design/10-services.md). Pages and components inject only `WeatherService`. Only `WeatherService` injects `HttpClient`. Open-Meteo's JSON never leaves the `weather` module.

### How a dashboard loads

```k
@Load let place: Place = try await weather.place(id: placeId)
@Load let forecast: Forecast = try await weather.forecast(place: place, units: units)
@Load let airQuality: AirQuality = try await weather.airQuality(place: place)
```

`forecast` and `airQuality` read `place`, so the compiler starts them when `place` resolves, and runs them in parallel with each other. `loading` renders until all three have resolved. `failed` receives a `WeatherError`, and the page matches on it exhaustively: not found, network (with a nested `match` on the `HttpError`), or an invalid response. Every ten minutes `reload()` re-runs the graph. The cached place returns at once, and the dashboard stays on screen until the new forecast arrives.

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
| `weather/WeatherService.test.k` | Mapping search, place, forecast and air-quality responses. Query parameters per unit system. The two-character minimum. Place caching, and forecasts not being cached. Errors: not found (400), server error (503), timeout, offline, malformed JSON, series of different lengths, an unknown time zone, and a missing AQI. |
| `weather/WeatherCondition.test.k` | WMO code groups, showers, unknown and negative codes, day and night icons |
| `weather/AirQuality.test.k` | Every category boundary, values above the scale, missing and negative indexes |
| `weather/Place.test.k` | `displayName` with repeated or missing parts |
| `components/CitySearch.test.k` | The idle hint, the debounce (299 ms vs 300 ms, and restarting on new input), the one-letter minimum, the in-progress text, no matches, failure, and clearing the field |
| `pages/CityWeather.test.k` | The loaded dashboard, the `loading` block, parallel loading after the place, imperial units from the parameter, not found, timeout with a working retry, 5xx vs 4xx messages, an invalid response, refreshing every 10 minutes, keeping the view during a refresh, and stopping the timer on unmount |
| `pages/Home.test.k` | The search field and featured links |

## Design proposals this example relies on

The design docs don't yet specify these, so the example uses the proposals in [`docs/design/proposals`](../../docs/design/proposals). Each is marked "Status: proposal".

| Proposal | Used for |
|---|---|
| [`doc-comments.md`](../../docs/design/proposals/doc-comments.md) | `///` doc comments with `- Parameter:`, `- Returns:`, `- Throws:` sections |
| [`k-toml.md`](../../docs/design/proposals/k-toml.md) | [`k.toml`](k.toml) and the fixed project layout |
| [`app-structure.md`](../../docs/design/proposals/app-structure.md) | [`App.k`](App.k): `public app`, `@Layout` on the app, app-wide `metadata` |
| [`http-client.md`](../../docs/design/proposals/http-client.md) | `HttpClient`, `HttpRequest`, `HttpError`, `fetchJson`, `@JsonKey` |
| [`charts.md`](../../docs/design/proposals/charts.md) | `LineChart`, `LineSeries`, `ChartPoint`, chart tokens |
| [`timers-and-clock.md`](../../docs/design/proposals/timers-and-clock.md) | `TimerHandle.cancel()`, `Timer.sleep`, `Clock`, `TestClock` |
| [`load-reload.md`](../../docs/design/proposals/load-reload.md) | `reload()`, keeping the view during a refresh |
| [`catch-binding.md`](../../docs/design/proposals/catch-binding.md) | `catch let error` for wrapping at a layer boundary |
| [`testing-support.md`](../../docs/design/proposals/testing-support.md) | The `testing` module, `service FakeX implements X`, `@Inject` of a fake, `screen.settle()`, `fill`, `unmount` |

A few smaller stdlib APIs are assumed and not specified anywhere yet:

- `formatted(as: .wholeNumber)` and `formatted(as: .relative)` beside the documented `.currency`
- `Date(unixSeconds:)` and `TimeZone(identifier:)`
- `Angle(degrees:)`
- `String.trimmed()`
- `Icon(name:)` with Lucide-style icon names
