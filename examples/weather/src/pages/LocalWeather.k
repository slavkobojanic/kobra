import components.AirQualityPanel
import components.CurrentConditionsCard
import components.TemperatureChart
import weather.AirQuality
import weather.Forecast
import weather.Place
import weather.UnitSystem
import weather.WeatherService

/// The dashboard for the device's position, opened by "Use my location":
/// `/local-weather/52.52/13.41`.
///
/// Open-Meteo has no reverse geocoding, so the position has no place name;
/// the forecast and the air quality need only its coordinates. A URL can't
/// carry a `Double`, so they are text, read by `Place.here`; text that
/// isn't a number fails the load. The page isn't remembered among the
/// recently viewed places, which are named.
public page LocalWeather {
    @Param let latitude: String
    @Param let longitude: String
    @Param let units: UnitSystem = .metric

    @Inject let weather: WeatherService

    @Load let place: Place = try Place.here(latitude: latitude, longitude: longitude)
    @Load let forecast: Forecast = try await weather.forecast(place: place, units: units)
    @Load let airQuality: AirQuality = try await weather.airQuality(place: place)

    metadata {
        title("Weather at your location")
    }

    view {
        heading(level: 1) { "Your location" }

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

    failed {
        section {
            heading(level: 1) { "Weather unavailable" }

            paragraph { "Open-Meteo could not answer for your location. Please try again later." }

            button { "Try again" }
                .on(click: reload)
        }
        .class(.status)
    }

    style {
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
