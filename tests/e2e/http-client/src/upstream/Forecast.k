/// The part of the fake's forecast this app reads.
public struct Forecast implements Codable {
    @JsonKey("temperature_2m") let temperature: Double
    let unit: String
}
