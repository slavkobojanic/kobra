/// A band of the European Air Quality Index, as defined by the European
/// Environment Agency.
public enum AirQualityCategory implements Codable, Equatable {
    /// Index 0 to 19.
    case good

    /// Index 20 to 39.
    case fair

    /// Index 40 to 59.
    case moderate

    /// Index 60 to 79.
    case poor

    /// Index 80 to 99.
    case veryPoor

    /// Index 100 and above.
    case extremelyPoor

    /// No index is available for the place, or the reported value is invalid.
    case unavailable

    /// A short English description, for example `"Very poor"`.
    var label: String {
        match self {
            case .good { "Good" }

            case .fair { "Fair" }

            case .moderate { "Moderate" }

            case .poor { "Poor" }

            case .veryPoor { "Very poor" }

            case .extremelyPoor { "Extremely poor" }

            case .unavailable { "Unavailable" }
        }
    }
}
