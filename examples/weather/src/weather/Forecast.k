import k.time.TimeZone

/// Current conditions and the next 24 hours at one place.
///
/// A `Forecast` is a snapshot. `WeatherService` never caches it, and
/// `CityWeather` replaces it with a fresh one every ten minutes.
public struct Forecast implements Codable, Equatable {
    /// The weather right now.
    public let current: CurrentConditions

    /// One reading per hour for the next 24 hours, earliest first.
    public let hourly: List<HourlyReading>

    /// The unit system every value in this forecast is expressed in.
    public let units: UnitSystem

    /// The place's time zone. Times are shown in it, not in the viewer's.
    public let timeZone: TimeZone

    /// When Open-Meteo last updated the current conditions.
    public let updatedAt: Date
}
