// Sample domain values shared by page and component tests.
//
// They are static members added by public extensions in this test-only
// module, so every test file sees them without an import and can write
// `.berlin` wherever a `Place` is expected (docs/design/21-testing.md,
// docs/design/16-modules.md). No production code can see them.

import k.time.TimeZone
import weather.AirQuality
import weather.CurrentConditions
import weather.Forecast
import weather.HourlyReading
import weather.Place
import weather.UnitSystem

public extend Place {
    /// Berlin, Germany. Its region differs from its name.
    static let berlin: Place = Place(
        id: 2950159,
        name: "Berlin",
        region: "Land Berlin",
        country: "Germany",
        latitude: 52.52437,
        longitude: 13.41053
    )

    /// Lisbon, Portugal. Its region repeats its name.
    static let lisbon: Place = Place(
        id: 2267057,
        name: "Lisbon",
        region: "Lisbon",
        country: "Portugal",
        latitude: 38.71667,
        longitude: -9.13333
    )
}

public extend Forecast {
    /// A forecast of mild, partly cloudy weather: 8 °C (46 °F) now and 24
    /// hourly readings starting at noon UTC on 6 November 2026.
    ///
    /// - Parameter units: the unit system to express the values in.
    static func sample(units: UnitSystem) -> Forecast {
        let start = Date(unixSeconds: 1793966400)
        let temperature = match units {
            case .metric { 8.0 }

            case .imperial { 46.4 }
        }

        return Forecast(
            current: CurrentConditions(
                temperature: temperature,
                feelsLike: temperature - 2.0,
                humidity: 81,
                windSpeed: 14.0,
                windDirection: 250.0,
                condition: .partlyCloudy,
                isDay: true
            ),
            hourly: (0..<24).map { hour =>
                HourlyReading(
                    time: start + hour.hours,
                    temperature: temperature,
                    feelsLike: temperature - 2.0,
                    precipitationChance: 10
                )
            },
            units: units,
            timeZone: TimeZone.utc,
            updatedAt: start
        )
    }
}

public extend AirQuality {
    /// Fair air: European AQI 32, with both particulate readings present.
    static let sample: AirQuality = AirQuality(europeanIndex: 32, pm25: 6.4, pm10: 11.0)
}
