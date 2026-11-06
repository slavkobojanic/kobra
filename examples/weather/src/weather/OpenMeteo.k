// Open-Meteo's wire format and its conversion into domain types.
//
// Nothing in this file is public. Pages and components only ever see `Place`,
// `Forecast` and `AirQuality`; if Open-Meteo changes its JSON, this file and
// `WeatherService.k` are the only code that changes.

import k.time.TimeZone

let geocodingSearchUrl = Url("https://geocoding-api.open-meteo.com/v1/search")
let geocodingLookupUrl = Url("https://geocoding-api.open-meteo.com/v1/get")
let forecastUrl = Url("https://api.open-meteo.com/v1/forecast")
let airQualityUrl = Url("https://air-quality-api.open-meteo.com/v1/air-quality")

let currentFields = "temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,wind_direction_10m,weather_code,is_day"
let hourlyFields = "temperature_2m,apparent_temperature,precipitation_probability"

extend UnitSystem {
    /// The value of Open-Meteo's `temperature_unit` query parameter.
    var temperatureParameter: String {
        match self {
            case .metric { "celsius" }

            case .imperial { "fahrenheit" }
        }
    }

    /// The value of Open-Meteo's `wind_speed_unit` query parameter.
    var windSpeedParameter: String {
        match self {
            case .metric { "kmh" }

            case .imperial { "mph" }
        }
    }
}

/// The body of `GET /v1/search`. Open-Meteo leaves out `results` entirely
/// when nothing matches.
struct GeocodingResponse implements Codable {
    let results: List<GeocodingResult>?
}

/// One place in a geocoding response, and the whole body of `GET /v1/get`.
struct GeocodingResult implements Codable {
    let id: Int
    let name: String
    let latitude: Double
    let longitude: Double
    let country: String?
    @JsonKey("admin1") let region: String?

    func toPlace() -> Place {
        return Place(
            id: id,
            name: name,
            region: region,
            country: country,
            latitude: latitude,
            longitude: longitude
        )
    }
}

/// The body of `GET /v1/forecast`, requested with `timeformat=unixtime`.
struct ForecastResponse implements Codable {
    let timezone: String
    let current: CurrentResponse
    let hourly: HourlyResponse

    /// Converts the response, checking what the JSON types cannot express:
    /// that parallel series have equal lengths and that the time zone exists.
    func toForecast(units: UnitSystem) throws WeatherError -> Forecast {
        guard let timeZone = TimeZone.parse(timezone) else {
            throw WeatherError.invalidResponse(reason: "unknown time zone \"{timezone}\"")
        }

        guard hourly.isConsistent else {
            throw WeatherError.invalidResponse(reason: "hourly series have different lengths")
        }

        return Forecast(
            current: current.toConditions(),
            hourly: hourly.toReadings(),
            units: units,
            timeZone: timeZone,
            updatedAt: Date(unixSeconds: current.time)
        )
    }
}

struct CurrentResponse implements Codable {
    let time: Int
    @JsonKey("temperature_2m") let temperature: Double
    @JsonKey("apparent_temperature") let feelsLike: Double
    @JsonKey("relative_humidity_2m") let humidity: Int
    @JsonKey("wind_speed_10m") let windSpeed: Double
    @JsonKey("wind_direction_10m") let windDirection: Double
    @JsonKey("weather_code") let weatherCode: Int
    @JsonKey("is_day") let isDay: Int

    func toConditions() -> CurrentConditions {
        return CurrentConditions(
            temperature: temperature,
            feelsLike: feelsLike,
            humidity: humidity,
            windSpeed: windSpeed,
            windDirection: windDirection,
            condition: conditionFor(wmoCode: weatherCode),
            isDay: isDay == 1
        )
    }
}

/// Hourly values arrive as parallel arrays: index `i` of every array
/// describes the hour starting at `time[i]`.
struct HourlyResponse implements Codable {
    let time: List<Int>
    @JsonKey("temperature_2m") let temperature: List<Double>
    @JsonKey("apparent_temperature") let feelsLike: List<Double>
    @JsonKey("precipitation_probability") let precipitationChance: List<Int>

    var isConsistent: Bool {
        let counts = [temperature.count, feelsLike.count, precipitationChance.count]

        return counts.all { count => count == time.count }
    }

    func toReadings() -> List<HourlyReading> {
        return (0..<time.count).map { index =>
            HourlyReading(
                time: Date(unixSeconds: time[index]),
                temperature: temperature[index],
                feelsLike: feelsLike[index],
                precipitationChance: precipitationChance[index]
            )
        }
    }
}

/// The body of `GET /v1/air-quality`.
struct AirQualityResponse implements Codable {
    let current: AirQualityCurrent
}

struct AirQualityCurrent implements Codable {
    @JsonKey("european_aqi") let europeanIndex: Int?
    @JsonKey("pm2_5") let pm25: Double?
    let pm10: Double?

    func toAirQuality() -> AirQuality {
        return AirQuality(europeanIndex: europeanIndex, pm25: pm25, pm10: pm10)
    }
}
