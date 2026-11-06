import components.AirQualityPanel
import components.CurrentConditionsCard
import components.TemperatureChart
import k.http.HttpError
import weather.AirQuality
import weather.Forecast
import weather.Place
import weather.UnitSystem
import weather.WeatherService

/// The dashboard for one place: current conditions, air quality and the
/// next 24 hours.
///
/// The three `@Load` fields form a dependency graph that the compiler turns
/// into a loading order. `place` reads only the `placeId` parameter, so it
/// loads first. `forecast` and `airQuality` both read `place`, so they wait
/// for it and then run in parallel with each other. Changing `units` re-runs
/// only `forecast`, because it is the only load that reads `units`.
///
/// While the page is open, the data is reloaded every ten minutes. A reload
/// keeps the current values on screen until the new ones arrive.
public page CityWeather {
    /// The GeoNames id of the place. Required, so it is a path segment:
    /// `/city-weather/2950159`.
    @Param let placeId: Int

    /// The unit system. Optional, so it is a query parameter that is left out
    /// of the URL when it is `.metric`: `/city-weather/2950159?units=imperial`.
    @Param let units: UnitSystem = .metric

    @Inject let weather: WeatherService

    @Load let place: Place = try await weather.place(id: placeId)
    @Load let forecast: Forecast = try await weather.forecast(place: place, units: units)
    @Load let airQuality: AirQuality = try await weather.airQuality(place: place)

    private let refreshInterval: Duration = 10.minutes

    /// Restarts the refresh timer whenever the page shows a different place,
    /// and stops it when the page unmounts.
    @Effect(on: [placeId])
    func refreshPeriodically() {
        let timer = Timer.every(refreshInterval) { reload() }

        onCleanup { timer.cancel() }
    }

    /// A sentence explaining a transport failure to the person using the app.
    func explain(error: HttpError) -> String {
        return match error {
            case .offline { "You appear to be offline." }

            case .timeout { "Open-Meteo took too long to respond." }

            case .status(let code, _) where code >= 500 { "Open-Meteo is having problems (error {code})." }

            case .status(let code, _) { "Open-Meteo could not answer this request (error {code})." }

            case .decoding { "Open-Meteo sent data the app could not read." }
        }
    }

    metadata {
        title("{place.name} weather")
        description("Current conditions, air quality and the next 24 hours for {place.displayName}.")
        canonical(.cityWeather(placeId: placeId))
    }

    view {
        header {
            heading(level: 1) { place.displayName }

            nav {
                link(to: .cityWeather(placeId: placeId)) { "°C" }
                    .class([.unit, .selected when units == .metric])

                link(to: .cityWeather(placeId: placeId).units(.imperial)) { "°F" }
                    .class([.unit, .selected when units == .imperial])
            }
            .label("Temperature unit")
        }
        .class(.header)

        box {
            CurrentConditionsCard(current: forecast.current, units: units, updatedAt: forecast.updatedAt)

            AirQualityPanel(airQuality: airQuality)
        }
        .class(.summary)

        TemperatureChart(hours: forecast.hourly, units: units, timeZone: forecast.timeZone)
    }

    loading {
        paragraph { "Loading forecast…" }
            .class(.status)
    }

    failed(let error) {
        section {
            match error {
                case .placeNotFound(let id) {
                    heading(level: 1) { "Place not found" }

                    paragraph { "There is no place with id {id}. Try searching for it instead." }

                    link(to: .home) { "Search for a place" }
                }

                case .network(let cause) {
                    heading(level: 1) { "Weather unavailable" }

                    paragraph { explain(error: cause) }

                    button { "Try again" }
                        .on(click: reload)
                }

                case .invalidResponse {
                    heading(level: 1) { "Weather unavailable" }

                    paragraph { "Open-Meteo sent an unexpected response. Please try again later." }

                    button { "Try again" }
                        .on(click: reload)
                }
            }
        }
        .class(.status)
    }

    style {
        .header {
            display: flex;
            flexWrap: wrap;
            alignItems: baseline;
            justifyContent: space-between;
            gap: .md;
        }

        .unit {
            padding: .xs .sm;
            borderRadius: .md;
            color: .muted;
            textDecoration: none;
        }

        .selected {
            color: .surface;
            backgroundColor: .primary;
        }

        .summary {
            display: grid;
            gridTemplateColumns: 2fr 1fr;
            gap: .lg;

            @media (maxWidth: 720px) { gridTemplateColumns: 1fr; }
        }

        .status {
            display: grid;
            gap: .md;
            justifyItems: start;
            padding: .xl 0;
            color: .muted;
        }
    }
}
