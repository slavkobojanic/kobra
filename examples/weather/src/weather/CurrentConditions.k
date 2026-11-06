/// The weather at a place right now.
///
/// All values are in the `UnitSystem` the forecast was requested in.
public struct CurrentConditions implements Codable, Equatable {
    /// Air temperature two metres above ground.
    public let temperature: Double

    /// The apparent ("feels like") temperature, combining wind chill and humidity.
    public let feelsLike: Double

    /// Relative humidity, from 0 to 100.
    public let humidity: Int

    /// Wind speed ten metres above ground.
    public let windSpeed: Double

    /// The direction the wind blows from, in degrees clockwise from north.
    public let windDirection: Double

    /// The kind of weather.
    public let condition: WeatherCondition

    /// Whether the sun is up at the place.
    public let isDay: Bool
}
