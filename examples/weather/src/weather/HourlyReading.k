/// The forecast for one hour.
public struct HourlyReading implements Codable, Equatable {
    /// The start of the hour.
    public let time: Date

    /// Air temperature two metres above ground.
    public let temperature: Double

    /// The apparent ("feels like") temperature.
    public let feelsLike: Double

    /// The probability of precipitation during the hour, from 0 to 100.
    public let precipitationChance: Int
}
