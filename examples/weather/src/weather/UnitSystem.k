/// The measurement system used to request and display a forecast.
///
/// `UnitSystem` is a page parameter on `CityWeather`, so it appears in the URL
/// as `?units=imperial` and is omitted when it is the default, `.metric`.
public enum UnitSystem implements Codable, Equatable {
    /// Degrees Celsius and kilometres per hour.
    case metric

    /// Degrees Fahrenheit and miles per hour.
    case imperial

    /// The symbol shown after a temperature: `"°C"` or `"°F"`.
    var temperatureSymbol: String {
        match self {
            case .metric { "°C" }

            case .imperial { "°F" }
        }
    }

    /// The unit shown after a wind speed: `"km/h"` or `"mph"`.
    var windSpeedSymbol: String {
        match self {
            case .metric { "km/h" }

            case .imperial { "mph" }
        }
    }

    /// Formats a temperature in this system, rounded to a whole degree.
    ///
    /// - Parameter value: the temperature, already expressed in this system's unit.
    /// - Returns: the formatted value with its symbol, for example `"8°C"`.
    func temperature(value: Double) -> String {
        return "{value.formatted(as: .wholeNumber)}{temperatureSymbol}"
    }

    /// Formats a wind speed in this system, rounded to a whole number.
    ///
    /// - Parameter value: the speed, already expressed in this system's unit.
    /// - Returns: the formatted value with its unit, for example `"14 km/h"`.
    func windSpeed(value: Double) -> String {
        return "{value.formatted(as: .wholeNumber)} {windSpeedSymbol}"
    }
}
