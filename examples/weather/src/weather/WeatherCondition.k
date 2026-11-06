import k.ui.IconName

/// The kind of weather, reduced from the WMO weather interpretation codes that
/// Open-Meteo reports.
///
/// The WMO table distinguishes about thirty codes (light, moderate and dense
/// drizzle, and so on). The dashboard only needs to tell them apart at a
/// glance, so related codes share a case. Codes outside the table map to
/// `.unknown` instead of failing the whole forecast.
public enum WeatherCondition implements Codable, Equatable {
    /// Clear sky (WMO 0).
    case clear

    /// Mainly clear or partly cloudy (WMO 1–2).
    case partlyCloudy

    /// Overcast (WMO 3).
    case overcast

    /// Fog or depositing rime fog (WMO 45, 48).
    case fog

    /// Drizzle of any intensity, including freezing drizzle (WMO 51–57).
    case drizzle

    /// Rain or rain showers of any intensity, including freezing rain (WMO 61–67, 80–82).
    case rain

    /// Snow, snow grains or snow showers (WMO 71–77, 85–86).
    case snow

    /// Thunderstorm, with or without hail (WMO 95–99).
    case thunderstorm

    /// A code that is not in the WMO table.
    case unknown

    /// A short English description, for example `"Partly cloudy"`.
    var label: String {
        match self {
            case .clear { "Clear" }

            case .partlyCloudy { "Partly cloudy" }

            case .overcast { "Overcast" }

            case .fog { "Fog" }

            case .drizzle { "Drizzle" }

            case .rain { "Rain" }

            case .snow { "Snow" }

            case .thunderstorm { "Thunderstorm" }

            case .unknown { "Unknown" }
        }
    }

    /// The icon for this condition from the bundled `k.ui` icon set.
    ///
    /// - Parameter isDay: whether the sun is up at the place. Clear and partly
    ///   cloudy skies show the moon at night; other conditions look the same.
    /// - Returns: the icon to pass to `Icon(name:)`.
    func icon(isDay: Bool) -> IconName {
        return match self {
            case .clear where isDay { .sun }

            case .clear { .moon }

            case .partlyCloudy where isDay { .cloudSun }

            case .partlyCloudy { .cloudMoon }

            case .overcast { .cloud }

            case .fog { .cloudFog }

            case .drizzle { .cloudDrizzle }

            case .rain { .cloudRain }

            case .snow { .snowflake }

            case .thunderstorm { .cloudLightning }

            case .unknown { .circleHelp }
        }
    }
}

/// Maps a WMO weather interpretation code to a `WeatherCondition`.
///
/// See the "WMO Weather interpretation codes" table in the Open-Meteo
/// forecast API documentation. Showers are reported as rain or snow, because
/// the dashboard does not distinguish showers from steady precipitation.
func conditionFor(wmoCode: Int) -> WeatherCondition {
    return match wmoCode {
        case 0 { .clear }

        case 1, 2 { .partlyCloudy }

        case 3 { .overcast }

        case 45, 48 { .fog }

        case 51, 53, 55, 56, 57 { .drizzle }

        case 61, 63, 65, 66, 67, 80, 81, 82 { .rain }

        case 71, 73, 75, 77, 85, 86 { .snow }

        case 95, 96, 99 { .thunderstorm }

        default { .unknown }
    }
}
