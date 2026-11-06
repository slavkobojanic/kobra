/// Current air pollution at a place.
///
/// Open-Meteo's air quality model does not cover every location, so each
/// value is optional. A missing index is not an error: the dashboard shows
/// the category as `.unavailable` and keeps the rest of the forecast.
public struct AirQuality implements Codable, Equatable {
    /// The European Air Quality Index, from 0 upwards, when available.
    public let europeanIndex: Int?

    /// Particulate matter smaller than 2.5 µm, in µg/m³, when available.
    public let pm25: Double?

    /// Particulate matter smaller than 10 µm, in µg/m³, when available.
    public let pm10: Double?

    /// The band `europeanIndex` falls into, or `.unavailable` when there is
    /// no index or the index is negative.
    public var category: AirQualityCategory {
        guard let index = europeanIndex else {
            return .unavailable
        }

        guard index >= 0 else {
            return .unavailable
        }

        return match index {
            case 0..<20 { .good }

            case 20..<40 { .fair }

            case 40..<60 { .moderate }

            case 60..<80 { .poor }

            case 80..<100 { .veryPoor }

            default { .extremelyPoor }
        }
    }
}
